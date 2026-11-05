import('../css/styles.scss');
import * as bootstrap from 'bootstrap';
import('leaflet/dist/leaflet.css');
import * as L from 'leaflet/dist/leaflet.js';
import('leaflet-draw/dist/leaflet.draw.css');
import('leaflet-draw/dist/leaflet.draw.js');

// The bundler rewrites the leaflet CSS marker image to a hashed URL, so
// leaflet's default icon path detection fails and falls back to the bare
// relative filenames ("marker-icon.png" etc). Those resolve against the
// current route directory, which breaks on any route with a path segment
// (e.g. /view/locations) since the images are served from the root.
// Point the default icon at the root, where the marker images are shipped.
L.Icon.Default.mergeOptions({ imagePath: '/' });

L.Control.Button = L.Control.extend({
    options: {
        position: 'topleft'
    },
    onAdd: function (map) {
        var container = L.DomUtil.create('div', 'leaflet-bar leaflet-control');
        var button = L.DomUtil.create('a', 'leaflet-control-button', container);
        button.innerHTML = '<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" fill="currentColor" class="bi bi-list" viewBox="0 0 16 16"><path fill-rule="evenodd" d="M2.5 12a.5.5 0 0 1 .5-.5h10a.5.5 0 0 1 0 1H3a.5.5 0 0 1-.5-.5m0-4a.5.5 0 0 1 .5-.5h10a.5.5 0 0 1 0 1H3a.5.5 0 0 1-.5-.5m0-4a.5.5 0 0 1 .5-.5h10a.5.5 0 0 1 0 1H3a.5.5 0 0 1-.5-.5"/></svg>';
        L.DomEvent.disableClickPropagation(button);
        L.DomEvent.on(button, 'click', function () {
            console.log('click');
            map.fire('show_locations');
        });

        container.title = "Robotica";

        return container;
    },
    onRemove: function (map) {},
});

L.control.button = function (opts) {
    return new L.Control.Button(opts);
};

import("../../pkg/robotica_frontend.js").catch(console.error);