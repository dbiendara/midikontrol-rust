import tkinter as tk
from tkinter import ttk, messagebox
import json, subprocess, os, threading
import mido

CONFIG_PATH = os.path.expanduser("~/.config/midicontrol/config.json")
SERVICE_NAME = "midicontrol.service"

# Mapping von MIDI-IDs zu lesbaren Namen
LED_NAMES = {
    32: "Ch 1 - Solo", 33: "Ch 2 - Solo", 34: "Ch 3 - Solo", 35: "Ch 4 - Solo",
    36: "Ch 5 - Solo", 37: "Ch 6 - Solo", 38: "Ch 7 - Solo", 39: "Ch 8 - Solo",
    48: "Ch 1 - Mute", 49: "Ch 2 - Mute", 50: "Ch 3 - Mute", 51: "Ch 4 - Mute",
    52: "Ch 5 - Mute", 53: "Ch 6 - Mute", 54: "Ch 7 - Mute", 55: "Ch 8 - Mute",
    64: "Ch 1 - Rec", 65: "Ch 2 - Rec", 66: "Ch 3 - Rec", 67: "Ch 4 - Rec",
    68: "Ch 5 - Rec", 69: "Ch 6 - Rec", 70: "Ch 7 - Rec", 71: "Ch 8 - Rec",
    41: "Play", 42: "Stop", 43: "Rewind", 44: "Fast Forward", 45: "Record", 46: "Cycle",
    58: "Track <", 59: "Track >",
    60: "Marker Set", 61: "Marker <", 62: "Marker >"
}

class MidiConfigApp:
    def __init__(self, root):
        self.root = root
        self.root.title("nanoKONTROL2 Visual Configurator")
        self.root.geometry("1300x750")
        
        self.load_config()
        self.refresh_system_audio()
        self.setup_ui()

    def load_config(self):
        try:
            with open(CONFIG_PATH, "r") as f:
                self.config = json.load(f)
            if "static_leds" not in self.config: self.config["static_leds"] = []
            if "mappings" not in self.config: self.config["mappings"] = {}
        except:
            self.config = {"enable_blinking": True, "static_leds": [], "mappings": {}}

    def refresh_system_audio(self):
        try:
            sinks = subprocess.check_output(["pactl", "list", "sinks", "short"], text=True)
            self.system_sinks = [line.split('\t')[1] for line in sinks.strip().split('\n') if line]
            sources = subprocess.check_output(["pactl", "list", "sources", "short"], text=True)
            self.system_sources = [line.split('\t')[1] for line in sources.strip().split('\n') if line]
            clients = subprocess.check_output(["sh", "-c", "pactl list clients | grep 'application.name =' | cut -d'\"' -f2"], text=True)
            self.running_apps = sorted(list(set(line.strip() for line in clients.split('\n') if line.strip())))
            for default in ["spotify", "firefox", "vivaldi", "mpv", "discord"]:
                if default not in self.running_apps: self.running_apps.append(default)
        except:
            self.system_sinks = []; self.system_sources = []; self.running_apps = ["spotify", "firefox"]

    def save_and_restart(self):
        with open(CONFIG_PATH, "w") as f:
            json.dump(self.config, f, indent=2)
        subprocess.run(["systemctl", "--user", "restart", SERVICE_NAME])
        messagebox.showinfo("Erfolg", "Konfiguration gespeichert und Dienst neu gestartet.")

    def setup_ui(self):
        for widget in self.root.winfo_children(): widget.destroy()
        top = ttk.Frame(self.root, padding=10); top.pack(fill="x")
        ttk.Button(top, text="Anlern-Modus (Service STOP)", command=self.start_learn_mode).pack(side="left")
        
        leg = ttk.Frame(top); leg.pack(side="left", padx=20)
        for txt, col, fg in [("Mapping", "#2e7d32", "white"), ("Statisch", "#1565c0", "white"), ("Beides", "#6a1b9a", "white"), ("Blink-Indikator", "#ff8f00", "black")]:
             tk.Label(leg, text=txt, bg=col, fg=fg, font=('Arial', 8), padx=5).pack(side="left", padx=2)

        self.blink_var = tk.BooleanVar(value=self.config.get("enable_blinking", True))
        ttk.Checkbutton(top, text="Globales Blinken", variable=self.blink_var, 
                        command=lambda: self.config.update({"enable_blinking": self.blink_var.get()})).pack(side="left", padx=10)
        ttk.Button(top, text="Speichern & Dienst START", command=self.save_and_restart).pack(side="right")

        main_container = ttk.Frame(self.root, padding=10); main_container.pack(fill="both", expand=True)
        left_side = ttk.Frame(main_container); left_side.pack(side="left", fill="y", padx=(0, 15))
        tc_frame = ttk.LabelFrame(left_side, text="Track / Cycle", padding=5); tc_frame.pack(fill="x", pady=(0, 10))
        track_row = ttk.Frame(tc_frame); track_row.pack(fill="x")
        self.create_ctrl(track_row, 58, "Track <", side="left", expand=True)
        self.create_ctrl(track_row, 59, "Track >", side="left", expand=True)
        self.create_ctrl(tc_frame, 46, "Cycle", fill="x")
        mkr = ttk.LabelFrame(left_side, text="Marker", padding=5); mkr.pack(fill="x", pady=5)
        for mid, name in [(60, "Set"), (61, "<"), (62, ">")]: self.create_ctrl(mkr, mid, name, side="left", expand=True)
        tra = ttk.LabelFrame(left_side, text="Transport", padding=5); tra.pack(fill="x", pady=5, side="bottom")
        for mid, name in [(43,"<<"),(44,">>"),(42,"Stop"),(41,"Play"),(45,"Rec")]: 
            self.create_ctrl(tra, mid, name, side="left", expand=True)

        chan_frame = ttk.Frame(main_container); chan_frame.pack(side="left", fill="both", expand=True)
        for i in range(8):
            col = ttk.LabelFrame(chan_frame, text=f"Ch {i+1}", padding=5); col.grid(row=0, column=i, padx=3, sticky="nsew")
            self.create_ctrl(col, 16+i, "Knob"); self.create_ctrl(col, i, "Fader")
            self.create_ctrl(col, 32+i, "S"); self.create_ctrl(col, 48+i, "M"); self.create_ctrl(col, 64+i, "R")

    def create_ctrl(self, parent, midi_id, label, **pack_opts):
        mid_str = str(midi_id); has_map = mid_str in self.config["mappings"]; is_stat = midi_id in self.config["static_leds"]
        is_blink_target = any(m.get("blink_led") == midi_id for m in self.config["mappings"].values())
        color = "#424242"; fg_color = "white"; hl_color = None
        if has_map and is_stat: color = "#6a1b9a"
        elif has_map: color = "#2e7d32"
        elif is_stat: color = "#1565c0"
        if is_blink_target:
            if color == "#424242": color = "#ff8f00"; fg_color = "black"
            else: hl_color = "#ff8f00"
        display_text = f"{label}\n{midi_id}" + ("\n[B]" if is_blink_target else "")
        btn = tk.Button(parent, text=display_text, bg=color, fg=fg_color, font=('Arial', 8, 'bold'), command=lambda: self.edit_mapping(midi_id))
        if hl_color: btn.config(highlightbackground=hl_color, highlightthickness=3)
        if not pack_opts: pack_opts = {"fill": "x", "padx": 1, "pady": 1}
        else: pack_opts.setdefault("padx", 1); pack_opts.setdefault("pady", 1)
        btn.pack(**pack_opts)

    def edit_mapping(self, midi_id):
        mid_str = str(midi_id)
        current = self.config["mappings"].get(mid_str, {"type": "None", "target": ""})
        win = tk.Toplevel(self.root); win.title(f"Konfig ID {midi_id}"); win.geometry("500x650"); win.transient(self.root)
        ttk.Label(win, text=f"ID {midi_id} ({LED_NAMES.get(midi_id, 'Control')})", font=('Arial', 10, 'bold')).pack(pady=10)
        static_var = tk.BooleanVar(value=midi_id in self.config["static_leds"])
        ttk.Checkbutton(win, text="Statisch leuchten (Dauerlicht)", variable=static_var).pack(pady=5)
        ttk.Separator(win, orient='horizontal').pack(fill='x', padx=20, pady=10)

        ttk.Label(win, text="Funktion (Typ):").pack()
        type_var = tk.StringVar(value=current["type"])
        type_cb = ttk.Combobox(win, textvariable=type_var, 
                               values=["App", "MuteApp", "Sink", "Source", "MuteSink", "MuteSource", "Media", "None"], 
                               state="readonly")
        type_cb.pack(pady=5)

        ttk.Label(win, text="Ziel (Target):").pack()
        container = ttk.Frame(win); container.pack(fill="both", expand=True, padx=40, pady=5)
        target_var = tk.StringVar(value=current["target"])
        target_cb = ttk.Combobox(container, textvariable=target_var)
        
        list_frame = ttk.Frame(container); scroll = ttk.Scrollbar(list_frame, orient="vertical")
        app_listbox = tk.Listbox(list_frame, selectmode="multiple", yscrollcommand=scroll.set, height=8)
        scroll.config(command=app_listbox.yview)

        def update_ui_elements(event=None):
            t = type_var.get()
            target_cb.pack_forget()
            list_frame.pack_forget()

            if t in ["App", "MuteApp"]:
                list_frame.pack(fill="both", expand=True)
                app_listbox.pack(side="left", fill="both", expand=True); scroll.pack(side="right", fill="y")
                app_listbox.delete(0, tk.END)
                current_targets = current["target"].split("|")
                for i, app in enumerate(self.running_apps):
                    app_listbox.insert(tk.END, app)
                    if app in current_targets: app_listbox.select_set(i)
            else:
                target_cb.pack(fill="x")
                if t in ["Sink", "MuteSink"]: target_cb['values'] = self.system_sinks
                elif t in ["Source", "MuteSource"]: target_cb['values'] = self.system_sources
                elif t == "Media": target_cb['values'] = ["play", "stop", "prev", "next", "defaultsink"]
                else: target_cb['values'] = []

        type_cb.bind("<<ComboboxSelected>>", update_ui_elements); update_ui_elements()

        ttk.Label(win, text="Blink-LED (optional):").pack(pady=(10, 0))
        blink_opts = ["Keine"] + [f"{name} (ID {id})" for id, name in LED_NAMES.items() if id != midi_id]
        blink_var = tk.StringVar()
        curr_blink = current.get("blink_led")
        blink_var.set(f"{LED_NAMES[curr_blink]} (ID {curr_blink})" if curr_blink in LED_NAMES else "Keine")
        ttk.Combobox(win, textvariable=blink_var, values=blink_opts, state="readonly").pack(pady=5)

        def save():
            if static_var.get():
                if midi_id not in self.config["static_leds"]: self.config["static_leds"].append(midi_id)
            elif midi_id in self.config["static_leds"]: self.config["static_leds"].remove(midi_id)

            if type_var.get() == "None": self.config["mappings"].pop(mid_str, None)
            else:
                if type_var.get() in ["App", "MuteApp"]:
                    final_target = "|".join([app_listbox.get(i) for i in app_listbox.curselection()])
                else:
                    final_target = target_var.get()
                
                new_map = {"type": type_var.get(), "target": final_target}
                sel_blink = blink_var.get()
                if sel_blink != "Keine": new_map["blink_led"] = int(sel_blink.split("ID ")[1].replace(")", ""))
                self.config["mappings"][mid_str] = new_map
            win.destroy(); self.setup_ui()

        ttk.Button(win, text="Übernehmen", command=save).pack(pady=20)

    def start_learn_mode(self):
        subprocess.run(["systemctl", "--user", "stop", SERVICE_NAME])
        threading.Thread(target=self.midi_listener, daemon=True).start()
        messagebox.showinfo("Anlern-Modus", "Bewege einen Regler oder drücke eine Taste am Gerät.")

    def midi_listener(self):
        try:
            port = next(n for n in mido.get_input_names() if "nanoKONTROL2" in n)
            with mido.open_input(port) as inport:
                for msg in inport:
                    if msg.type == 'control_change':
                        self.root.after(0, lambda m=msg: self.edit_mapping(m.control)); break
        except Exception as e: self.root.after(0, lambda: messagebox.showerror("Fehler", f"MIDI Fehler: {e}"))

if __name__ == "__main__":
    root = tk.Tk(); app = MidiConfigApp(root); root.mainloop()