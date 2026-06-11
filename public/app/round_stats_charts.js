function renderRoundStatsColor(color, rounds, tokenSymbol) {
  const container = $(`${color}RoundStatsCharts`);
  const count = $(`${color}RoundStatsCount`);
  if (!container) {
    return;
  }

  container.replaceChildren();
  if (count) {
    count.textContent = rounds.length ? `${rounds.length} rounds` : "empty";
  }

  for (const chart of ROUND_STATS_CHARTS) {
    container.appendChild(roundStatsChartCard(chart, rounds, tokenSymbol));
  }
}

function renderRoundStatsLoading() {
  state.roundStatsRenderKey = null;
  for (const color of ["blue", "green"]) {
    const container = $(`${color}RoundStatsCharts`);
    const count = $(`${color}RoundStatsCount`);
    if (count) {
      count.textContent = "loading";
    }
    if (container) {
      container.replaceChildren(roundStatsStatus("Loading statistics"));
    }
  }
}

function renderRoundStatsError(error) {
  state.roundStatsRenderKey = null;
  for (const color of ["blue", "green"]) {
    const container = $(`${color}RoundStatsCharts`);
    const count = $(`${color}RoundStatsCount`);
    if (count) {
      count.textContent = "unavailable";
    }
    if (container) {
      container.replaceChildren(roundStatsStatus(roundStatsErrorMessage(error)));
    }
  }
}

function roundStatsErrorMessage(error) {
  const message = String(error?.message || error || "");
  if (message.includes("timeout")) {
    return "Statistics request timed out.";
  }
  return "Statistics are unavailable.";
}

function roundStatsStatus(message) {
  const status = document.createElement("div");
  status.className = "round-stats-status";
  status.textContent = message;
  return status;
}

function roundStatsChartCard(chart, rounds, tokenSymbol) {
  const card = document.createElement("section");
  card.className = `round-stats-chart-card is-${chart.key}`;

  const header = document.createElement("div");
  header.className = "round-stats-chart-header";
  const title = document.createElement("h3");
  title.textContent = chart.title;
  const latest = document.createElement("strong");
  latest.textContent = chart.latest(rounds.at(-1), tokenSymbol);
  header.append(title, latest);

  const body = document.createElement("div");
  body.className = "round-stats-chart-body";
  body.appendChild(roundStatsSvg(chart, rounds));

  card.append(header, body);
  return card;
}

function roundStatsSvg(chart, rounds) {
  const svg = createSvg("svg");
  svg.setAttribute("viewBox", "0 0 360 176");
  svg.setAttribute("role", "img");
  svg.setAttribute("aria-label", chart.title);
  svg.classList.add("round-stats-chart");

  const plot = { left: 47, top: 12, right: 14, bottom: 38 };
  const width = 360 - plot.left - plot.right;
  const height = 176 - plot.top - plot.bottom;
  const allValues = chart.series
    .flatMap((series) => rounds.map((round) => series.value(round)))
    .filter(roundStatsFinite);
  const scale = roundStatsScale(allValues, chart.unit);
  const xFor = (index) => plot.left + (rounds.length === 1 ? width / 2 : width * index / (rounds.length - 1));
  const yFor = (value) => plot.top + (scale.max - value) / (scale.max - scale.min) * height;

  appendRoundStatsGrid(svg, plot, width, height, scale, chart.unit);

  chart.series.forEach((series, seriesIndex) => {
    const points = rounds
      .map((round, index) => {
        const value = series.value(round);
        if (!roundStatsFinite(value)) {
          return null;
        }
        return { x: xFor(index), y: yFor(value), value, round };
      })
      .filter(Boolean);
    if (!points.length) {
      return;
    }

    const line = createSvg("polyline");
    line.setAttribute("points", points.map((point) => `${point.x.toFixed(2)},${point.y.toFixed(2)}`).join(" "));
    line.classList.add("round-stats-line", `series-${seriesIndex + 1}`);
    svg.appendChild(line);

    for (const point of points) {
      const dot = createSvg("circle");
      dot.setAttribute("cx", point.x.toFixed(2));
      dot.setAttribute("cy", point.y.toFixed(2));
      dot.setAttribute("r", "3.2");
      dot.classList.add("round-stats-dot", `series-${seriesIndex + 1}`);
      svg.appendChild(dot);

      const hitArea = createSvg("circle");
      hitArea.setAttribute("cx", point.x.toFixed(2));
      hitArea.setAttribute("cy", point.y.toFixed(2));
      hitArea.setAttribute("r", "9");
      hitArea.setAttribute("tabindex", "0");
      hitArea.setAttribute("aria-label", roundStatsTooltipLabel(series, point.round));
      hitArea.classList.add("round-stats-hit-area");
      setValidatorTooltip(hitArea, roundStatsTooltipLines(series, point.round));
      svg.appendChild(hitArea);
    }
  });

  appendRoundStatsXAxis(svg, plot, width, rounds);
  return svg;
}

function appendRoundStatsGrid(svg, plot, width, height, scale, unit) {
  const ticks = [scale.max, (scale.min + scale.max) / 2, scale.min];
  ticks.forEach((value, index) => {
    const y = plot.top + height * index / 2;
    const line = createSvg("line");
    line.setAttribute("x1", String(plot.left));
    line.setAttribute("x2", String(plot.left + width));
    line.setAttribute("y1", y.toFixed(2));
    line.setAttribute("y2", y.toFixed(2));
    line.classList.add("round-stats-grid-line");
    svg.appendChild(line);

    const label = createSvg("text");
    label.setAttribute("x", String(plot.left - 8));
    label.setAttribute("y", String(y + 3));
    label.setAttribute("text-anchor", "end");
    label.classList.add("round-stats-axis-label");
    label.textContent = roundStatsAxisLabel(value, unit);
    svg.appendChild(label);
  });

  const axis = createSvg("path");
  axis.setAttribute("d", `M${plot.left} ${plot.top}V${plot.top + height}H${plot.left + width}`);
  axis.classList.add("round-stats-axis");
  svg.appendChild(axis);
}

function appendRoundStatsXAxis(svg, plot, width, rounds) {
  const baseline = 176 - plot.bottom + 11;
  rounds.forEach((round, index) => {
    const x = plot.left + (rounds.length === 1 ? width / 2 : width * index / (rounds.length - 1));
    const tick = createSvg("line");
    tick.setAttribute("x1", x.toFixed(2));
    tick.setAttribute("x2", x.toFixed(2));
    tick.setAttribute("y1", String(176 - plot.bottom));
    tick.setAttribute("y2", String(176 - plot.bottom + 4));
    tick.classList.add("round-stats-axis-tick");
    svg.appendChild(tick);

    const label = createSvg("text");
    label.setAttribute("x", x.toFixed(2));
    label.setAttribute("y", String(baseline + 12));
    label.setAttribute("text-anchor", index === 0 ? "start" : index === rounds.length - 1 ? "end" : "middle");
    label.classList.add("round-stats-x-label");
    label.textContent = String(round.round_id);
    svg.appendChild(label);
  });
}

function roundStatsScale(values, unit) {
  if (!values.length) {
    return { min: 0, max: 1 };
  }

  let min = Math.min(...values);
  let max = Math.max(...values);
  if (min === max) {
    const pad = unit === "count" ? 1 : Math.max(Math.abs(max) * 0.01, unit === "percent" ? 0.1 : 1);
    min -= pad;
    max += pad;
  } else {
    const pad = (max - min) * 0.16;
    min -= pad;
    max += pad;
  }

  if (unit === "count") {
    min = Math.floor(min);
    max = Math.ceil(max);
    if (min === max) {
      max += 1;
    }
  }

  return { min, max };
}

function roundStatsTooltipLabel(series, round) {
  return `Round ${round.round_id}, ${series.label}: ${series.tooltip(round)}`;
}

function roundStatsTooltipLines(series, round) {
  return [
    `Round: ${round.round_id}`,
    `${series.label}: ${series.tooltip(round)}`,
  ];
}

function createSvg(tagName) {
  return document.createElementNS("http://www.w3.org/2000/svg", tagName);
}
