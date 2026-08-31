// Node location statistics: the cards, tables, and ranking markup.
function nodeStatsCardHtml(label, value, detail, tooltip = "", featured = false, extraClass = "") {
  const className = ["node-stats-card", featured ? "is-featured" : "", extraClass].filter(Boolean).join(" ");
  return `
    <div class="${escapeHtml(className)}"${tooltip ? ` data-node-stats-tooltip="${escapeHtml(tooltip)}"` : ""}>
      <span>${escapeHtml(label)}</span>
      <strong>${escapeHtml(value)}</strong>
      ${detail ? `<small>${escapeHtml(detail)}</small>` : ""}
    </div>
  `;
}

function wireNodeStatsCardTooltips(root) {
  for (const card of root.querySelectorAll("[data-node-stats-tooltip]")) {
    setValidatorTooltip(card, card.dataset.nodeStatsTooltip || "");
  }
}

function hideNodeStatsTooltip() {
  if (typeof hideValidatorTooltip === "function") {
    hideValidatorTooltip();
  }
}

function wireNodeStatsTableScrollHints(root) {
  const shells = Array.from(root.querySelectorAll(".node-stats-table-shell"));
  const updateShell = (shell) => {
    const maxScrollLeft = Math.max(0, shell.scrollWidth - shell.clientWidth);
    const hasMore = maxScrollLeft > 1 && shell.scrollLeft < maxScrollLeft - 1;
    shell.classList.toggle("has-scroll-more", hasMore);
  };

  for (const shell of shells) {
    shell.addEventListener("scroll", () => updateShell(shell), { passive: true });
    window.requestAnimationFrame(() => updateShell(shell));
  }
}

function nodeStatsBlockTitleHtml(label, icon) {
  return `
    <h3 class="node-stats-block-title">
      <span class="node-stats-block-icon node-stats-icon-${escapeHtml(icon)}" aria-hidden="true">
        ${nodeStatsBlockIconSvg(icon)}
      </span>
      <span>${escapeHtml(label)}</span>
    </h3>
  `;
}

function nodeStatsBlockIconSvg(icon) {
  if (icon === "countries") {
    return `
      <svg viewBox="0 0 24 24" focusable="false">
        <circle cx="12" cy="12" r="8.2"></circle>
        <path d="M3.8 12h16.4"></path>
        <path d="M12 3.8a12.2 12.2 0 0 1 0 16.4"></path>
        <path d="M12 3.8a12.2 12.2 0 0 0 0 16.4"></path>
      </svg>
    `;
  }
  if (icon === "isp") {
    return `
      <svg viewBox="0 0 24 24" focusable="false">
        <circle cx="7" cy="8" r="2"></circle>
        <circle cx="17" cy="8" r="2"></circle>
        <circle cx="12" cy="17" r="2"></circle>
        <path d="M8.7 9.2 11 15.1"></path>
        <path d="m15.3 9.2-2.3 5.9"></path>
        <path d="M9 8h6"></path>
      </svg>
    `;
  }
  if (icon === "city") {
    return `
      <svg viewBox="0 0 24 24" focusable="false">
        <path d="M5 19V7l5-2v14"></path>
        <path d="M10 19V9l5-2v12"></path>
        <path d="M15 19v-7l4 1.8V19"></path>
        <path d="M4 19h16"></path>
      </svg>
    `;
  }
  return `
    <svg viewBox="0 0 24 24" focusable="false">
      <path d="M8 21h8"></path>
      <path d="M12 17v4"></path>
      <path d="M7 4h10v3a5 5 0 0 1-10 0Z"></path>
      <path d="M7 6H4a3 3 0 0 0 3 3"></path>
      <path d="M17 6h3a3 3 0 0 1-3 3"></path>
    </svg>
  `;
}

function wireNodeStatsRankingToggle(root) {
  const button = root.querySelector("[data-node-stats-ranking-toggle]");
  if (!button) {
    return;
  }

  button.addEventListener("click", () => {
    const block = button.closest(".node-stats-block-placement");
    if (!block) {
      return;
    }
    const expanded = block.classList.toggle("is-ranking-expanded");
    state.nodeStatsLocationRankingExpanded = expanded;
    const summary = block.querySelector("[data-node-stats-ranking-summary]");
    button.textContent = nodeStatsRankingActionText(expanded);
    button.setAttribute("aria-expanded", expanded ? "true" : "false");
    if (summary) {
      summary.textContent = nodeStatsRankingSummaryText(
        Number(summary.dataset.visibleCount || 0),
        Number(summary.dataset.totalCount || 0),
        expanded,
      );
    }
  });
}

function nodeStatsCountryTableHtml(rows) {
  return nodeStatsAggregateTableHtml(rows, NODE_STATS_LABELS.columns.country, "country", "countries");
}

function nodeStatsRankTableHtml(rows) {
  return nodeStatsAggregateTableHtml(rows, NODE_STATS_LABELS.columns.cluster, "cluster", "clusters");
}

function nodeStatsAggregateTableHtml(rows, nameHeader, singularLabel, pluralLabel) {
  const tableRows = nodeStatsVisibleRows(rows, singularLabel, pluralLabel);
  return `
    <div class="node-stats-table-shell">
      <table class="node-stats-table">
        <colgroup>
          <col class="node-stats-col-rank">
          <col class="node-stats-col-name">
          <col class="node-stats-col-count">
          <col class="node-stats-col-stake">
          <col class="node-stats-col-percent">
        </colgroup>
        <thead>
          <tr>
            <th scope="col">${escapeHtml(NODE_STATS_LABELS.columns.rank)}</th>
            <th scope="col">${escapeHtml(nameHeader)}</th>
            <th scope="col">${escapeHtml(NODE_STATS_LABELS.columns.nodes)}</th>
            <th scope="col">${escapeHtml(NODE_STATS_LABELS.columns.stake)}</th>
            <th scope="col">${escapeHtml(NODE_STATS_LABELS.columns.weightPercent)}</th>
          </tr>
        </thead>
        <tbody>
          ${tableRows.map((row, index) => nodeStatsAggregateRowHtml(row, index)).join("")}
        </tbody>
      </table>
    </div>
  `;
}

function nodeStatsAggregateRowHtml(row, index) {
  return `
    <tr${row.isRemainder ? ` class="is-remainder"` : ""}>
      <td>${formatNodeStatsInteger(index + 1)}</td>
      <td>${escapeHtml(row.label)}</td>
      <td>${formatNodeStatsInteger(row.nodes)}</td>
      <td>${formatNodeStatsStake(row.stake)}</td>
      <td>${formatPercent(row.weightPercent)}</td>
    </tr>
  `;
}

function nodeStatsVisibleRows(rows, singularLabel, pluralLabel) {
  const visibleRows = rows.slice(0, NODE_STATS_VISIBLE_ROWS);
  const remainderRows = rows.slice(NODE_STATS_VISIBLE_ROWS);
  if (!remainderRows.length) {
    return visibleRows;
  }

  const label = remainderRows.length === 1 ? singularLabel : pluralLabel;
  return [
    ...visibleRows,
    {
      label: `Other ${formatNodeStatsInteger(remainderRows.length)} ${label}`,
      nodes: remainderRows.reduce((sum, row) => sum + row.nodes, 0),
      stake: remainderRows.reduce((sum, row) => sum + row.stake, 0),
      stakePercent: remainderRows.reduce((sum, row) => sum + row.stakePercent, 0),
      weightPercent: remainderRows.reduce((sum, row) => sum + row.weightPercent, 0),
      isRemainder: true,
    },
  ];
}

function nodeStatsPlacementHtml(stats) {
  const mappedLocations = stats.mappedLocationRows;
  const visibleCount = Math.min(5, mappedLocations.length);
  const extraCount = Math.max(0, mappedLocations.length - visibleCount);
  const expanded = isNodeStatsLocationRankingExpanded();
  return `
    <div class="node-stats-placement">
      <div class="node-stats-table-shell">
        <table class="node-stats-table node-stats-ranking-table">
          <colgroup>
            <col class="node-stats-col-rank">
            <col class="node-stats-col-location">
            <col class="node-stats-col-distance-primary">
            <col class="node-stats-col-distance-secondary">
            <col class="node-stats-col-distance-tertiary">
          </colgroup>
          <thead>
            <tr>
              <th scope="col">${escapeHtml(NODE_STATS_LABELS.columns.rank)}</th>
              <th scope="col">${escapeHtml(NODE_STATS_LABELS.columns.mappedLocation)}</th>
              <th scope="col">${escapeHtml(NODE_STATS_LABELS.columns.weightedAverage)}</th>
              <th scope="col">${escapeHtml(NODE_STATS_LABELS.columns.median)}</th>
              <th scope="col">${escapeHtml(NODE_STATS_LABELS.columns.p90)}</th>
            </tr>
          </thead>
          <tbody>
            ${mappedLocations.map((row, index) => `
            <tr${nodeStatsPlacementRowClass(index, visibleCount)}>
              <td>${formatNodeStatsInteger(index + 1)}</td>
              <td>${escapeHtml(row.label)}</td>
              <td>${formatNodeStatsDistance(row.weightedAverageKm)}</td>
              <td>${formatNodeStatsDistance(row.medianKm)}</td>
              <td>${formatNodeStatsDistance(row.p90Km)}</td>
            </tr>
            `).join("")}
          </tbody>
        </table>
      </div>
      ${extraCount ? `
      <div class="node-stats-ranking-footer">
        <span data-node-stats-ranking-summary data-visible-count="${visibleCount}" data-total-count="${mappedLocations.length}">${escapeHtml(nodeStatsRankingSummaryText(visibleCount, mappedLocations.length, expanded))}</span>
        <button class="node-stats-ranking-action" type="button" data-node-stats-ranking-toggle aria-expanded="${expanded ? "true" : "false"}">${escapeHtml(nodeStatsRankingActionText(expanded))}</button>
      </div>
      ` : ""}
    </div>
  `;
}

function nodeStatsPlacementBlockClass() {
  return [
    "node-stats-block",
    "node-stats-block-placement",
    isNodeStatsLocationRankingExpanded() ? "is-ranking-expanded" : "",
  ].filter(Boolean).join(" ");
}

function isNodeStatsLocationRankingExpanded() {
  return Boolean(state.nodeStatsLocationRankingExpanded);
}

function nodeStatsRankingActionText(expanded) {
  return expanded ? NODE_STATS_LABELS.actions.showTopFive : NODE_STATS_LABELS.actions.viewFullRanking;
}

function nodeStatsRankingSummaryText(visibleCount, totalCount, expanded) {
  const visible = Number.isFinite(visibleCount) ? Math.max(0, Math.trunc(visibleCount)) : 0;
  const total = Number.isFinite(totalCount) ? Math.max(0, Math.trunc(totalCount)) : 0;
  if (expanded) {
    return `Showing all ${formatNodeStatsInteger(total)}`;
  }
  return `Top ${formatNodeStatsInteger(visible)} of ${formatNodeStatsInteger(total)}`;
}

function nodeStatsPlacementRowClass(index, visibleCount) {
  const classes = [];
  if (index === 0) {
    classes.push("is-best");
  }
  if (index >= visibleCount) {
    classes.push("is-extra-ranking");
  }
  return classes.length ? ` class="${classes.join(" ")}"` : "";
}


function formatNodeStatsRoundColor(value) {
  const color = String(value || "").trim();
  return color ? `${color.slice(0, 1).toUpperCase()}${color.slice(1).toLowerCase()}` : "";
}

function formatNodeStatsStake(value) {
  return formatTokenAmount(value, 0, 0);
}

function formatNodeStatsInteger(value) {
  return Number(value || 0).toLocaleString(undefined, { maximumFractionDigits: 0 });
}

function formatNodeStatsDistance(value) {
  return `${Math.round(Number(value || 0)).toLocaleString()} km`;
}
