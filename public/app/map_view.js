function resetValidatorMapView(duration = 450) {
  if (!validatorMap) {
    return;
  }

  closeValidatorMapPopups();
  validatorMap.fitBounds(VALIDATOR_MAP_DEFAULT_BOUNDS, {
    ...VALIDATOR_MAP_DEFAULT_OPTIONS,
    duration
  });
}
