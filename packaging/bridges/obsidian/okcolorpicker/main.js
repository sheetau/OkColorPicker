const {MarkdownView, Plugin, PluginSettingTab, Setting} = require("obsidian");

const hex = /^#[0-9a-f]{6}$/i;
const canvasColors = ["", "#FB464C", "#E9973F", "#E0DE71", "#44CF6E", "#53DFDD", "#A882FF"];

function colorAtCursor(editor) {
    const cursor = editor.getCursor();
    const line = editor.getLine(cursor.line);
    const pattern = /#[0-9a-f]{6}(?![0-9a-f])/gi;
    for (const match of line.matchAll(pattern)) {
        const start = match.index;
        if (cursor.ch >= start && cursor.ch <= start + 7) {
            return {value: match[0], from: {line: cursor.line, ch: start}, to: {line: cursor.line, ch: start + 7}};
        }
    }
}

class ColorSettings extends PluginSettingTab {
    constructor(app, bridge) { super(app, bridge); this.bridge = bridge; }
    display() {
        this.containerEl.empty();
        new Setting(this.containerEl).setName("Apply picker to selected text")
            .addDropdown(control => control.addOptions({off:"Only when requested", color:"Text color", "background-color":"Background color"})
                .setValue(this.bridge.mode).onChange(async mode => {
                    this.bridge.mode = mode;
                    await this.bridge.saveData({mode});
                }));
    }
}

module.exports = class ColorBridge extends Plugin {
    async onload() {
        this.mode = (await this.loadData())?.mode || "off";
        this.color = null;
        this.previous = null;
        this.socket = null;
        this.enabled = true;
        this.addSettingTab(new ColorSettings(this.app, this));
        const actions = [["color", "Color selected text"], ["background-color", "Highlight selected text"]];
        for (const [prop, title] of actions) {
            this.addCommand({id: prop, name: title, editorCheckCallback: (checking, editor) => {
                if (!this.color || !editor.somethingSelected()) return false;
                if (!checking) this.wrap(editor, prop);
                return true;
            }});
        }
        this.registerEvent(this.app.workspace.on("editor-menu", (menu, editor) => {
            if (this.color && editor.somethingSelected()) for (const [prop, title] of actions) {
                menu.addItem(item => item.setTitle(title + " with OkColorPicker").onClick(() => this.wrap(editor, prop)));
            }
        }));
        this.registerInterval(window.setInterval(() => this.poll(), 100));
        this.connect();
    }

    connect() {
        if (!this.enabled) return;
        const socket = this.socket = new WebSocket("ws://127.0.0.1:9010");
        socket.onopen = () => { this.previous = null; this.color = null; };
        socket.onmessage = event => {
            let data;
            try { data = JSON.parse(event.data); } catch { return; }
            if (typeof data?.hex !== "string" || !hex.test(data.hex)) return;
            const value = data.hex.toUpperCase();
            this.color = value;
            this.apply(value);
        };
        socket.onclose = () => {
            this.color = null;
            if (this.enabled) this.retry = window.setTimeout(() => this.connect(), 1200);
        };
    }

    onunload() {
        this.enabled = false;
        window.clearTimeout(this.retry);
        if (this.socket) {
            this.socket.onmessage = this.socket.onclose = null;
            this.socket.close();
        }
    }

    wrap(editor, property) {
        const selected = editor.getSelection();
        if (!selected || !this.color) return;
        const start = editor.getCursor("from");
        const prefix = '<span style="' + property + ': ';
        const from = {line: start.line, ch: start.ch + prefix.length};
        editor.replaceSelection(prefix + this.color + '">' + selected + '</span>');
        editor.setSelection(from, {line: from.line, ch: from.ch + 7});
        this.previous = this.color;
    }

    canvas() {
        const view = this.app.workspace.activeLeaf?.view;
        return view?.getViewType() === "canvas" ? view.canvas : null;
    }

    poll() {
        if (this.socket?.readyState !== WebSocket.OPEN || !this.color) return;
        const editor = this.app.workspace.getActiveViewOfType(MarkdownView)?.editor;
        const canvas = this.canvas();
        let value;
        if (editor) value = colorAtCursor(editor)?.value;
        else if (canvas?.selection?.size === 1) {
            const item = canvas.selection.values().next().value;
            const c = item.getData?.().color ?? item.color;
            value = hex.test(c) ? c : canvasColors[Number(c)];
        }
        value = typeof value === "string" && hex.test(value) ? value.toUpperCase() : null;
        if (value && value !== this.previous) {
            this.color = value;
            this.socket.send(JSON.stringify({hex: value}));
        }
        this.previous = value;
    }

    apply(value) {
        const editor = this.app.workspace.getActiveViewOfType(MarkdownView)?.editor;
        if (editor) {
            const token = colorAtCursor(editor);
            if (token) {
                if (token.value.toUpperCase() !== value) {
                    const cursor = editor.getCursor();
                    editor.replaceRange(value, token.from, token.to);
                    editor.setCursor(cursor);
                }
                this.previous = value;
            } else if (editor.somethingSelected() && ["color","background-color"].includes(this.mode)) {
                this.wrap(editor, this.mode);
            }
            return;
        }
        const canvas = this.canvas();
        let changed = false;
        for (const item of canvas?.selection || []) {
            if (typeof item.setColor === "function" && (item.getData?.().color ?? item.color) !== value) {
                item.setColor(value);
                changed = true;
            }
        }
        if (changed) canvas.requestSave?.();
        this.previous = value;
    }
};
