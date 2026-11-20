// Executed by the host's ExtendScript engine. Keep syntax compatible with ES3.
function applyColors(program, job) {
    function nameOf(rgb) {
        var hex = (0x1000000 + rgb[0] * 65536 + rgb[1] * 256 + rgb[2]).toString(16).slice(1);
        return "OkColorPicker #" + hex.toUpperCase();
    }
    function rgbColor(rgb) {
        var color = program === "Photoshop" ? new SolidColor() : new RGBColor();
        var channels = program === "Photoshop" ? color.rgb : color;
        channels.red = rgb[0];
        channels.green = rgb[1];
        channels.blue = rgb[2];
        return color;
    }
    function photoshopSwatch(rgb) {
        var id = charIDToTypeID;
        var target = new ActionReference();
        target.putClass(id("Clrs"));
        var make = new ActionDescriptor();
        make.putReference(id("null"), target);
        var swatch = new ActionDescriptor();
        swatch.putString(id("Nm  "), nameOf(rgb));
        var value = new ActionDescriptor();
        value.putDouble(id("Rd  "), rgb[0]);
        value.putDouble(id("Grn "), rgb[1]);
        value.putDouble(id("Bl  "), rgb[2]);
        swatch.putObject(id("Clr "), id("RGBC"), value);
        make.putObject(id("Usng"), id("Clrs"), swatch);
        executeAction(id("Mk  "), make, DialogModes.NO);
    }
    function illustratorItem(item, slot, color) {
        if (!item || item.locked || item.hidden) return;
        var property = slot ? "strokeColor" : "fillColor";
        switch (item.typename) {
        case "CompoundPathItem":
            if (item.pathItems.length) illustratorItem(item.pathItems[0], slot, color);
            break;
        case "GroupItem":
            for (var i = 0; i < item.pageItems.length; ++i) illustratorItem(item.pageItems[i], slot, color);
            break;
        case "PathItem":
            item[slot ? "stroked" : "filled"] = true;
            item[property] = color;
            break;
        case "TextFrame":
            item.textRange.characterAttributes[property] = color;
            break;
        case "TextRange":
            item.characterAttributes[property] = color;
            break;
        }
    }
    function indesignSwatch(doc, name, rgb) {
        var swatch = doc.colors.itemByName(name);
        if (!swatch.isValid) return doc.colors.add({name:name,model:ColorModel.PROCESS,space:ColorSpace.RGB,colorValue:rgb});
        swatch.properties = {model:ColorModel.PROCESS,space:ColorSpace.RGB,colorValue:rgb};
        return swatch;
    }
    if (program === "Illustrator" && !app.documents.length) return "WAIT";
    var doc = app.documents.length ? app.activeDocument : null;
    for (var slot = 0; slot < 2; ++slot) {
        var rgb = job.slots[slot];
        if (!rgb) continue;
        if (program === "Photoshop") {
            app[slot ? "backgroundColor" : "foregroundColor"] = rgbColor(rgb);
        } else if (program === "Illustrator") {
            var color = rgbColor(rgb);
            doc[slot ? "defaultStrokeColor" : "defaultFillColor"] = color;
            var selected = doc.selection;
            if (selected && selected.typename === "TextRange") illustratorItem(selected, slot, color);
            else if (selected) for (var i = 0; i < selected.length; ++i) illustratorItem(selected[i], slot, color);
        } else {
            var swatch = indesignSwatch(doc || app, slot ? "OkColorPicker BG" : "OkColorPicker FG", rgb);
            var property = slot ? "strokeColor" : "fillColor";
            if (doc) doc.pageItemDefaults[property] = swatch;
            for (var j = 0; doc && j < app.selection.length; ++j) {
                var item = app.selection[j];
                if (item.isValid && !item.locked && property in item) item[property] = swatch;
            }
        }
    }
    for (var n = 0; job.palette && n < job.palette.length; ++n) {
        var entry = job.palette[n];
        if (program === "Photoshop") photoshopSwatch(entry);
        else if (program === "InDesign") indesignSwatch(doc || app, nameOf(entry), entry);
        else {
            var sample;
            try { sample = doc.swatches.getByName(nameOf(entry)); }
            catch (missing) { sample = doc.swatches.add(); sample.name = nameOf(entry); }
            sample.color = rgbColor(entry);
        }
    }
    if (program === "Illustrator") app.redraw();
    return "OK";
}
