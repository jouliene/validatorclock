function boundsForFeatures(features) {
  const bounds = new maplibregl.LngLatBounds();
  let hasCoordinates = false;
  for (const feature of Array.isArray(features) ? features : []) {
    const coordinates = feature?.geometry?.coordinates;
    if (!Array.isArray(coordinates) || coordinates.length < 2) {
      continue;
    }
    const lon = Number(coordinates[0]);
    const lat = Number(coordinates[1]);
    if (!Number.isFinite(lat) || !Number.isFinite(lon)) {
      continue;
    }
    bounds.extend([lon, lat]);
    hasCoordinates = true;
  }
  return hasCoordinates ? bounds : null;
}

function clusterFitPadding() {
  const compact = validatorMap?.getContainer()?.clientWidth < 700;
  return compact
    ? { top: 70, right: 48, bottom: 70, left: 48 }
    : { top: 92, right: 110, bottom: 92, left: 110 };
}

function clusterLeavesAreTight(features) {
  const bounds = boundsForFeatures(features);
  if (!bounds) {
    return false;
  }
  const west = bounds.getWest();
  const east = bounds.getEast();
  const south = bounds.getSouth();
  const north = bounds.getNorth();
  return Math.abs(east - west) <= 0.0002 && Math.abs(north - south) <= 0.0002;
}
