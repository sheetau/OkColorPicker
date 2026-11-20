from krita import Krita
from .bridge import PickerExtension

Krita.instance().addExtension(PickerExtension(Krita.instance()))
