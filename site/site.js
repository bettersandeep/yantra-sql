// The playground. The engine is lekha.wasm. This page loads no other code.
// The button "Ask the Pi" sends the SQL to the same engine on a Raspberry Pi.

const MAX_SQL = 2000; // Bytes. wasm/src/lib.rs has the same limit.
// The address of the engine on the Raspberry Pi. It is the server of server/.
const PI_URL = "https://lekha.bettersandeep.com/query";
const PI_TIMEOUT_MS = 5000;
const sql = document.getElementById("sql");
const result = document.getElementById("result");
const compiled = fetch("lekha.wasm")
  .then((response) => response.arrayBuffer())
  .then((bytes) => WebAssembly.compile(bytes));
let engine = null;
let newest = 0; // The number of the newest run.

// Returns { ok: true, rows } or { ok: false, error }.
async function execute(text) {
  const bytes = new TextEncoder().encode(text);
  if (bytes.length > MAX_SQL) {
    return { ok: false, error: `The SQL is longer than ${MAX_SQL} bytes.` };
  }
  try {
    engine ??= (await WebAssembly.instantiate(await compiled)).exports;
  } catch (error) {
    return { ok: false, error: `The engine did not load: ${error.message}` };
  }
  const { memory, alloc, dealloc, run } = engine;
  try {
    const input = alloc(bytes.length);
    new Uint8Array(memory.buffer, input, bytes.length).set(bytes);
    const packed = run(input, bytes.length);
    dealloc(input, bytes.length);
    const output = Number(packed >> 32n);
    const length = Number(packed & 0xffffffffn);
    const json = new TextDecoder().decode(new Uint8Array(memory.buffer, output, length));
    dealloc(output, length);
    return JSON.parse(json);
  } catch (error) {
    // A panic of the engine is a trap. The next run gets a new instance.
    engine = null;
    return { ok: false, error: `The engine stopped on this input: ${error.message}. This is a bug in lekha.` };
  }
}

// True when the value has the shape that server/README.md gives for POST /query:
// { ok: true, rows } where each value of a row is a string or null,
// or { ok: false, error } where the error is a string.
function isAnswer(value) {
  if (value === null || typeof value !== "object") return false;
  if (value.ok === false) return typeof value.error === "string";
  if (value.ok !== true || !Array.isArray(value.rows)) return false;
  return value.rows.every(
    (row) => Array.isArray(row) && row.every((cell) => cell === null || typeof cell === "string"),
  );
}

// Sends the SQL to the engine on the Raspberry Pi.
// Returns { answer, ms } with an answer in the form of execute(),
// or { reason } when the Pi did not answer.
// The body is a string, so the request is a simple CORS request with no preflight.
async function askPi(text) {
  const control = new AbortController();
  let late = false;
  // The limit covers the full request: the headers and the body of the response.
  const timer = setTimeout(() => {
    late = true;
    control.abort();
  }, PI_TIMEOUT_MS);
  const start = performance.now();
  try {
    const response = await fetch(PI_URL, {
      method: "POST",
      body: text,
      signal: control.signal,
      credentials: "omit",
      cache: "no-store",
    });
    if (response.status !== 200) return { reason: `the status was ${response.status}` };
    const body = await response.text();
    const ms = Math.round(performance.now() - start);
    let answer;
    try {
      answer = JSON.parse(body);
    } catch {
      return { reason: "the response was not JSON" };
    }
    if (!isAnswer(answer)) return { reason: "the response was not in the expected form" };
    return { answer, ms };
  } catch (error) {
    if (late) return { reason: `no answer in ${PI_TIMEOUT_MS / 1000} seconds` };
    return { reason: `the request failed (${error.message})` };
  } finally {
    clearTimeout(timer);
  }
}

// Shows the answer. Each text goes in as text, never as HTML.
// The note is an optional line of text below the answer.
function show(answer, note) {
  const parts = [];
  if (!answer.ok) {
    const error = document.createElement("p");
    error.className = "error";
    error.textContent = `Error: ${answer.error}`;
    parts.push(error);
  } else {
    const table = document.createElement("table");
    for (const row of answer.rows) {
      const tr = table.insertRow();
      for (const value of row) {
        const td = tr.insertCell();
        td.textContent = value ?? "NULL";
        if (value === null) td.className = "null";
      }
    }
    parts.push(table);
  }
  if (note !== undefined) {
    const source = document.createElement("p");
    source.className = "hint source";
    source.textContent = note;
    parts.push(source);
  }
  result.replaceChildren(...parts);
}

// Runs the SQL in the engine of the browser.
// Returns true when this run was still the newest one at its end, and so showed its answer.
async function run() {
  const number = ++newest;
  const answer = await execute(sql.value);
  // An older run that ends late does not replace the answer of a newer run.
  if (number !== newest) return false;
  show(answer);
  return true;
}

// Runs the SQL in the engine on the Raspberry Pi. Only the button "Ask the Pi" calls this.
async function ask() {
  const number = ++newest;
  const waiting = document.createElement("p");
  waiting.className = "hint source";
  waiting.textContent = "Asking the Raspberry Pi.";
  result.replaceChildren(waiting);
  const reply = await askPi(sql.value);
  if (number !== newest) return;
  if (reply.answer === undefined) {
    const error = document.createElement("p");
    error.className = "error";
    error.textContent = `The Raspberry Pi did not answer: ${reply.reason}.`;
    result.replaceChildren(error);
    return;
  }
  show(reply.answer, `This answer came from the Raspberry Pi. The request took ${reply.ms} ms.`);
}

document.getElementById("run").addEventListener("click", run);
document.getElementById("pi").addEventListener("click", ask);
sql.addEventListener("keydown", (event) => {
  if (event.key === "Enter" && (event.ctrlKey || event.metaKey)) {
    event.preventDefault();
    run();
  }
});
for (const button of document.querySelectorAll(".examples button")) {
  button.addEventListener("click", () => {
    sql.value = button.textContent;
    run();
  });
}

fetch("score.json")
  .then((response) => {
    if (!response.ok) throw new Error(`score.json: HTTP ${response.status}`);
    return response.json();
  })
  .then((score) => {
    const count = (n) => n.toLocaleString("en-US");
    const percent = ((100 * score.passed) / score.applicable).toFixed(1);
    const commit = document.createElement("a");
    commit.href = `https://github.com/bettersandeep/lekha/commit/${encodeURIComponent(score.generated_from_commit)}`;
    commit.textContent = score.generated_from_commit.slice(0, 7);
    document
      .getElementById("score")
      .append(
        `Score: ${count(score.passed)} of ${count(score.applicable)} records of SQLite's test file ${score.file} pass (${percent}%). Measured at commit `,
        commit,
        ".",
      );
  })
  .catch((error) => {
    console.error(error);
    document.getElementById("score").textContent = "The score did not load.";
  });

// On load, the page runs the SQL in the box.
// A test hook: each ?q=<sql> in the URL runs on load, in order.
// A run or a request to the Pi that the visitor starts ends this loop,
// so a later query of the link does not replace the visitor's SQL or answer.
(async () => {
  const queries = new URLSearchParams(location.search).getAll("q");
  for (const query of queries.length ? queries : [sql.value]) {
    sql.value = query;
    if (!(await run())) break;
  }
})();
