# Vendored map libraries

The map used to pull these from unpkg.com. A reader whose network cannot reach
that CDN - which is common behind a VPN, and common in the places people use one
from - got no error to act on: a script tag whose connection is black-holed
fires neither `load` nor `error`, so the map sat on "Loading map" for as long as
the page was open. Serving them ourselves removes the only third-party origin
the site had, and lets the content security policy say `script-src 'self'`.

Each file is byte-identical to the one in the package's npm tarball, and each
tarball was checked against the integrity hash the registry publishes for it:

    maplibre-gl@5.9.0  dist/maplibre-gl.js, dist/maplibre-gl.css   3-Clause BSD
    pmtiles@4.3.0      dist/pmtiles.js                             BSD-3-Clause

The upstream version is in the file name because these are served immutable for
a year: a new version has to arrive at a new URL or caches will not pick it up.

## Updating

    npm pack maplibre-gl@<version>          # or curl the registry tarball
    tar xzf maplibre-gl-<version>.tgz
    cp package/dist/maplibre-gl.js public/vendor/maplibre-gl-<version>.js

Then update `MAPLIBRE_JS_URL` and friends in `public/app/map.js` and the routes
in `src/server/routes.rs`. `vendored_map_libraries_are_served` fails if the two
disagree.
