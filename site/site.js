// The playground. The engine is lekha.wasm. This page loads no other code.

const MAX_SQL = 2000; // Bytes. wasm/src/lib.rs has the same limit.
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

// Shows the answer. Each text goes in as text, never as HTML.
async function run() {
  const number = ++newest;
  const answer = await execute(sql.value);
  // An older run that ends late does not replace the answer of a newer run.
  if (number !== newest) return;
  if (!answer.ok) {
    const error = document.createElement("p");
    error.className = "error";
    error.textContent = `Error: ${answer.error}`;
    result.replaceChildren(error);
    return;
  }
  const table = document.createElement("table");
  for (const row of answer.rows) {
    const tr = table.insertRow();
    for (const value of row) {
      const td = tr.insertCell();
      td.textContent = value ?? "NULL";
      if (value === null) td.className = "null";
    }
  }
  result.replaceChildren(table);
}

document.getElementById("run").addEventListener("click", run);
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
(async () => {
  const queries = new URLSearchParams(location.search).getAll("q");
  for (const query of queries.length ? queries : [sql.value]) {
    sql.value = query;
    await run();
  }
})();
