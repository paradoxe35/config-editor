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
 * The uses of a name in the files that are not open, as `onFindUses`
 * answers: late, as a host reading its disk or asking its server would.
 * `server.d/office.yaml` routes to `main`.
 */
export function findUses({ name }) {
  const uses =
    name === "main"
      ? [{ source: "server.d/office.yaml", path: "/routes/0/client", line: 5, text: "    client: main" }]
      : [];
  return new Promise((resolve) => setTimeout(() => resolve(uses), 400));
}
