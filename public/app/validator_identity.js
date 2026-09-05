function validatorIdentityCell(validator, options = {}, fallbackToPublicKey = false) {
  const cell = document.createElement("div");
  cell.className = "validator-cell validator-id";
  const identity = validatorIdentityDisplay(validator, options, fallbackToPublicKey);
  const address = copyableValue(identity.text, identity.value, "validator-address", identity.label);
  setValidatorTooltip(address, validatorIdentityTooltipLines(validator));
  cell.append(address);
  return cell;
}

// What names a validator in the table, and what the copy button hands over.
//
// A validator with no wallet is named by its public key, and a public key is not an address.
// Put through the address formatter, any 64-hex string comes out as `-1:<hex>`, and on TON it
// is encoded once more into a plausible-looking EQ... - an address that exists nowhere, shown
// to the reader, labelled "EVER address" by the tooltip and copied by the button. So the
// fallback is shown, and copied, as the key it is.
function validatorIdentityDisplay(validator, options = {}, fallbackToPublicKey = false) {
  if (validator?.wallet) {
    const formatted = formatDisplayAddress(validatorWalletAddress(validator), options);
    return { text: formatted.text, value: formatted.value, label: "validator wallet address" };
  }

  const publicKey = fallbackToPublicKey ? validator?.public_key || "" : "";
  if (!publicKey) {
    return { text: "-", value: "-", label: "validator wallet address" };
  }
  return { text: shortenHash(publicKey), value: publicKey, label: "validator public key" };
}

function validatorIdentityTooltipLines(validator) {
  return [
    `Validator Pubkey: ${validator?.public_key || "Unknown"}`,
    `Contract HASH: ${validator?.contract_type_hash || "Unknown"}`,
    `Type: ${validatorContractDisplayName(validator)}`,
  ];
}
