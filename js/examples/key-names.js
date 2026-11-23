// `?keys`: the documents pages' setup for key-name suggestions, for a server file
// and its drop-ins. Two schemas, each with an `$id`: the server's file and
// its drop-ins. Both define clients under `/clients`, and their fields that
// name a client (`default_client`, a route's `client`) take the names of
// either kind of file through `from`. `self` is a reserved word: an
// `examples` value, not a key. The host gives the names of every file once
// (as `keysAt` reads them); the open files' live names replace theirs.

export const keys = new URLSearchParams(location.search).has("keys");

const SERVER = "https://example.com/server";
const DROP_IN = "https://example.com/drop-in";
const clientName = {
  type: "string",
  examples: ["self"],
  "x-config-editor-keys": { pointer: "/clients", from: [SERVER, DROP_IN] },
};
const clients = {
  type: "object",
  additionalProperties: {
    type: "object",
    properties: { weight: { type: "integer" } },
  },
};

export const serverSchema = JSON.stringify({
  $id: SERVER,
  type: "object",
  properties: { listen: { type: "integer" }, default_client: clientName, clients },
});

export const dropInSchema = JSON.stringify({
  $id: DROP_IN,
  type: "object",
  properties: {
    clients,
    routes: {
      type: "array",
      items: {
        type: "object",
        properties: { path: { type: "string" }, client: clientName },
      },
    },
  },
});

/**
 * The files open at first: the server's, and two drop-ins. Their ids are
 * not their file names: the suggestions show the file name, as the tabs do.
 */
export const keyFiles = [
  {
    id: "server",
    fileName: "server.yaml",
    value: "listen: 7835\ndefault_client: \nclients:\n  main:\n    weight: 1\n",
    schema: serverSchema,
    closable: false,
  },
  {
    id: "home",
    fileName: "server.d/home.yaml",
    value: "clients:\n  home:\n    weight: 2\nroutes:\n  - path: /\n    client: \n",
    schema: dropInSchema,
  },
  {
    id: "lab",
    fileName: "server.d/lab.yaml",
    value: "clients:\n  lab:\n    weight: 1\n",
    schema: dropInSchema,
  },
];

/**
 * The host's names, read from every file on disk once, each with the id of
 * its document as its source: the open ones as saved (`home-saved` is gone
 * from the unsaved text), and one that is not open (its path as its id).
 */
export const keyNames = [
  { schema: SERVER, pointer: "/clients", names: [{ name: "main", source: "server" }] },
  {
    schema: DROP_IN,
    pointer: "/clients",
    names: [
      { name: "home-saved", source: "home" },
      { name: "office", source: "server.d/office.yaml", detail: "closed" },
    ],
  },
];

/**
 * The files on disk that are not open at first, their path as their id (as
 * the host's names and uses give it): opened as a tab when the user goes to
 * a definition or a use in one.
 */
export const closedFiles = [
  {
    id: "server.d/office.yaml",
    fileName: "server.d/office.yaml",
    value: "routes:\n  - path: /office\n    client: main\nclients:\n  office:\n    weight: 3\n",
    schema: dropInSchema,
  },
];

/**
 * The uses of a name in the files that are not open, as `onFindUses`
 * answers: late, as a host reading its disk or asking its server would.
 * `server.d/office.yaml` routes to `main`.
 */
export function findUses({ name }) {
  const uses =
    name === "main"
      ? [{ source: "server.d/office.yaml", path: "/routes/0/client", line: 3, text: "    client: main" }]
      : [];
  return new Promise((resolve) => setTimeout(() => resolve(uses), 400));
}

/**
 * The file to add to the open ones for a definition or a use in `source`
 * (`onGoToDefinition`, `onGoToUse`), when it is a closed file the host has:
 * a copy, which the host's list may change in place.
 */
export function closedFile(source, open) {
  if (open.some((file) => file.id === source)) return undefined;
  const file = closedFiles.find((candidate) => candidate.id === source);
  return file && { ...file };
}

/** Where a go-to target points in its file: a use's path, or the key a definition names. */
const pathOf = (target) =>
  target.path ?? `${target.pointer}/${target.name.replace(/~/g, "~0").replace(/\//g, "~1")}`;

/**
 * Shows the file of `target` (`onGoToDefinition`'s, `onGoToUse`'s) and
 * selects the field it points at, once the view lists the file (a
 * framework renders a new file a little later). `view()` is the documents
 * view, or a framework component's handle.
 */
export async function reveal(view, target) {
  if (!target.source) return false;
  let selected = false;
  for (let frame = 0; frame < 120; frame += 1) {
    const documents = view();
    selected ||= Boolean(documents?.select(target.source));
    if (selected && documents.editor(target.source)?.reveal(pathOf(target))) return true;
    await new Promise((resolve) => requestAnimationFrame(resolve));
  }
  return false;
}
