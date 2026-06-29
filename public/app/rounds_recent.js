function renderRecentRoundPanels(snapshot, model) {
  const grid = $("recentRoundsGrid");
  grid.replaceChildren();

  for (const color of ["blue", "green"]) {
    const round = displayedRoundForColor(color, snapshot, model);
    const validators = round?.recent_absent_validators || [];
    grid.appendChild(recentRoundPanel(color, validators, snapshot));
  }

  grid.hidden = false;
}

function displayedRoundForColor(color, snapshot, model) {
  if (snapshot.current_set.round_color === color) {
    return snapshot.current_set;
  }
  if (snapshot.next_set?.round_color === color) {
    return snapshot.next_set;
  }
  if (model.beforeElections && snapshot.previous_set?.round_color === color) {
    return snapshot.previous_set;
  }
  return null;
}

function recentRoundPanel(color, validators, snapshot) {
  const section = document.createElement("section");
  section.className = `recent-round-panel recent-${color}`;
  if (validators.length === 0) {
    section.classList.add("is-empty");
  }

  const heading = document.createElement("div");
  heading.className = "recent-round-heading";
  const title = document.createElement("h2");
  const roundParity = color === "blue" ? "even" : "odd";
  title.append(
    recentRoundTitleIcon(),
    document.createTextNode(`Seen in recent ${color} (${roundParity}) rounds`)
  );
  const count = document.createElement("span");
  count.className = "recent-round-count";
  count.textContent = validators.length === 0 ? "empty" : `${validators.length} absent now`;
  heading.append(title, count);
  section.appendChild(heading);

  const list = document.createElement("div");
  list.className = "validator-list";
  if (validators.length === 0) {
    const empty = document.createElement("div");
    empty.className = "recent-round-empty";
    empty.textContent = "No absent validators";
    list.appendChild(empty);
  } else {
    renderRecentAbsentValidators(list, validators, {
      ...validatorRenderOptions(snapshot),
      validatorSelectionScope: "recent",
      validatorSelectionColor: color,
    });
  }
  section.appendChild(list);

  return section;
}

function recentRoundTitleIcon() {
  const icon = document.createElement("span");
  icon.className = "recent-title-icon";
  icon.innerHTML = [
    '<svg viewBox="0 0 24 24" aria-hidden="true" focusable="false">',
    '<path d="M2.5 12s3.4-6.2 9.5-6.2 9.5 6.2 9.5 6.2-3.4 6.2-9.5 6.2S2.5 12 2.5 12Z"></path>',
    '<circle cx="12" cy="12" r="2.7"></circle>',
    '<path d="m17.8 17.8 3 3"></path>',
    '</svg>',
  ].join("");
  return icon;
}
