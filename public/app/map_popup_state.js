function trackValidatorMapPopup(popup) {
  validatorMapPopups.add(popup);
  popup.on("close", () => {
    validatorMapPopups.delete(popup);
  });
  return popup;
}

function closeValidatorMapPopups() {
  for (const popup of Array.from(validatorMapPopups)) {
    popup.remove();
  }
  validatorMapPopups.clear();
}
