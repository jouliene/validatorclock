function validatorSourceCell(validator, options = {}) {
  const cell = document.createElement("div");
  const sourceKind = validatorSourceKind(validator, options);
  cell.className = `validator-cell validator-source is-${sourceKind}`;

  const source = validator && validator.source;
  if (source && source.address) {
    const formatted = formatDisplayAddress(source.address, options);
    if (shouldDisplayTonSourceMetadata(validator, options)) {
      const meta = tonSourceMetadata(validator);
      const metadata = copyableValue(
        meta.label,
        formatted.value,
        `validator-source-address validator-source-meta is-${meta.className}`,
        "validator source address"
      );
      setValidatorTooltip(metadata, validatorSourceMetadataTooltipLines(validator, meta, source));
      cell.appendChild(metadata);
      return cell;
    }

    const address = copyableValue(
      formatted.text,
      formatted.value,
      "validator-source-address",
      "validator source address"
    );
    setValidatorTooltip(address, validatorSourceTooltipLines(validator, source));
    cell.appendChild(address);
    return cell;
  }

  if (isDirectValidatorContract(validator)) {
    const direct = document.createElement("span");
    direct.className = "validator-source-direct";
    direct.textContent = "Direct";
    setValidatorTooltip(direct, directValidatorTooltipLines(validator));
    cell.appendChild(direct);
    return cell;
  }

  const tonHash = tonValidatorContractHash(validator, options);
  if (tonHash) {
    const hash = copyableValue(
      shortenContractHash(tonHash),
      tonHash,
      "validator-source-address",
      "validator contract repr hash"
    );
    setValidatorTooltip(hash, `Contract HASH: ${tonHash}`);
    cell.appendChild(hash);
    return cell;
  }

  const unknown = document.createElement("span");
  unknown.className = "validator-source-unknown";
  unknown.textContent = "Unknown";
  cell.appendChild(unknown);
  return cell;
}
