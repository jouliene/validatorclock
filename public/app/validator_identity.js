function validatorIdentityCell(validator, options = {}, fallbackToPublicKey = false) {
  const cell = document.createElement("div");
  cell.className = "validator-cell validator-id";
  const identity = validatorIdentityValue(validator, fallbackToPublicKey);
  const formatted = formatDisplayAddress(identity, options);
  const address = copyableValue(formatted.text, formatted.value, "validator-address", "validator wallet address");
  setValidatorTooltip(address, validatorIdentityTooltipLines(validator));
  cell.append(address);
  return cell;
}

function validatorIdentityValue(validator, fallbackToPublicKey = false) {
  if (validator?.wallet) {
    return validatorWalletAddress(validator);
  }
  return fallbackToPublicKey ? (validator?.public_key || "-") : "-";
}

function validatorIdentityTooltipLines(validator) {
  return [
    `Validator Pubkey: ${validator?.public_key || "Unknown"}`,
    `Contract HASH: ${validator?.contract_type_hash || "Unknown"}`,
    `Type: ${validatorContractDisplayName(validator)}`,
  ];
}
