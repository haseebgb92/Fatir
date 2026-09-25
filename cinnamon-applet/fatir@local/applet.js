const Applet = imports.ui.applet;
const Util = imports.misc.util;

class FatirApplet extends Applet.IconApplet {
    constructor(metadata, orientation, panelHeight, instanceId) {
        super(orientation, panelHeight, instanceId);
        this.setAllowedLayout(Applet.AllowedLayout.BOTH);
        this.set_applet_icon_path(metadata.path + '/icon.png');
        this.set_applet_tooltip('Fatir Personal Assistant');
    }

    on_applet_clicked() {
        Util.spawnCommandLine("bash -lc 'command -v fatir >/dev/null && fatir || ~/.local/bin/fatir'");
    }
}

function main(metadata, orientation, panelHeight, instanceId) {
    return new FatirApplet(metadata, orientation, panelHeight, instanceId);
}
