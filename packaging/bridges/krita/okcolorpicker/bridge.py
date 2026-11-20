from threading import Event, Thread

from krita import Extension, Krita, ManagedColor, Palette, Swatch
try:
    from PyQt6.QtCore import QCoreApplication, QTimer
    from PyQt6.QtGui import QColor, QIcon
except ImportError:
    from PyQt5.QtCore import QCoreApplication, QTimer
    from PyQt5.QtGui import QColor, QIcon
from .transport import ColorLink


def rgb_bytes(values):
    return len(values) == 3 and all(type(x) is int and 0 <= x <= 255 for x in values)


class PickerExtension(Extension):
    def setup(self):
        self.link = ColorLink()
        self.stop = Event()
        self.seen = [None, None]
        self.worker = Thread(target=self.link.run, args=(self.stop,), daemon=True)
        self.worker.start()
        self.timer = QTimer(self)
        self.timer.timeout.connect(self.update_colors)
        self.timer.start(50)
        QCoreApplication.instance().aboutToQuit.connect(self.shutdown)

    def shutdown(self):
        self.timer.stop()
        self.stop.set()

    def createActions(self, window):
        pass

    def update_colors(self):
        window = Krita.instance().activeWindow()
        view = window.activeView() if window else None
        if not view or not view.document() or not self.link.connected:
            return
        getters = (view.foregroundColor, view.backgroundColor)
        setters = (view.setForeGroundColor, view.setBackGroundColor)
        canvas = view.canvas()
        for msg in self.link.take():
            if msg is None:
                self.seen = [None, None]
                continue
            if "palette" in msg:
                self.add_palette(view, msg["palette"])
                continue
            slot = msg.get("slot", 0)
            rgb = [msg.get(c) for c in "rgb"]
            if slot not in (0, 1) or not rgb_bytes(rgb):
                continue
            old = getters[slot]().colorForCanvas(canvas)
            setters[slot](ManagedColor.fromQColor(QColor(*rgb, old.alpha())))
            self.seen[slot] = getters[slot]().colorForCanvas(canvas).getRgb()[:3]
        for slot, getter in enumerate(getters):
            rgb = getter().colorForCanvas(canvas).getRgb()[:3]
            if self.seen[slot] is not None and self.seen[slot] != rgb:
                self.link.send(slot, rgb)
                self.seen[slot] = rgb

    def add_palette(self, view, colors):
        if not isinstance(colors, list) or not all(isinstance(c, list) and rgb_bytes(c) for c in colors):
            return
        resource = Krita.instance().resources("palette").get("OkColorPicker")
        if not resource:
            view.showFloatingMessage("Import the bundled OkColorPicker.gpl palette first.", QIcon(), 3000, 1)
            return
        palette = Palette(resource)
        for rgb in colors:
            swatch = Swatch()
            swatch.setColor(ManagedColor.fromQColor(QColor(*rgb)))
            swatch.setName("#%02X%02X%02X" % tuple(rgb))
            palette.addEntry(swatch, "")
        palette.save()
