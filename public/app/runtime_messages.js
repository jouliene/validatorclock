const networkMessageState = {
  messages: [],
  queue: [],
  last: "",
  timer: null,
  fadeMs: 1800,
  holdMs: 10000,
};

async function startNetworkMessages() {
  const element = $("networkMessageText");
  if (!element) {
    return;
  }

  window.clearTimeout(networkMessageState.timer);
  const fallback = element.textContent.trim();
  const messages = await loadNetworkMessages(fallback);
  networkMessageState.messages = messages;
  networkMessageState.queue = [];
  networkMessageState.last = "";
  showNextNetworkMessage(element);
}

async function loadNetworkMessages(fallback) {
  try {
    const data = await fetchJson(assetPath("/jokes.json"));
    const source = Array.isArray(data) ? data : Array.isArray(data.jokes) ? data.jokes : [];
    const messages = source
      .filter((message) => typeof message === "string")
      .map((message) => message.trim())
      .filter(Boolean);
    return messages.length ? messages : [fallback];
  } catch (error) {
    console.warn("Unable to load network messages", error);
    return fallback ? [fallback] : [];
  }
}

function showNextNetworkMessage(element) {
  const message = nextNetworkMessage();
  if (!message) {
    return;
  }

  element.classList.remove("is-visible", "is-exiting");
  element.textContent = message;
  void element.offsetWidth;
  element.classList.add("is-visible");

  networkMessageState.timer = window.setTimeout(() => {
    element.classList.add("is-exiting");
    element.classList.remove("is-visible");
    networkMessageState.timer = window.setTimeout(() => {
      showNextNetworkMessage(element);
    }, networkMessageState.fadeMs + 250);
  }, networkMessageState.holdMs);
}

function nextNetworkMessage() {
  if (!networkMessageState.messages.length) {
    return "";
  }

  if (!networkMessageState.queue.length) {
    networkMessageState.queue = shuffleNetworkMessages(networkMessageState.messages);
    if (
      networkMessageState.queue.length > 1
      && networkMessageState.queue[0] === networkMessageState.last
    ) {
      networkMessageState.queue.push(networkMessageState.queue.shift());
    }
  }

  const message = networkMessageState.queue.shift();
  networkMessageState.last = message;
  return message;
}

function shuffleNetworkMessages(messages) {
  const shuffled = [...messages];
  for (let index = shuffled.length - 1; index > 0; index -= 1) {
    const swapIndex = Math.floor(Math.random() * (index + 1));
    [shuffled[index], shuffled[swapIndex]] = [shuffled[swapIndex], shuffled[index]];
  }
  return shuffled;
}
