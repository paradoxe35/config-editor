// What the CSP pages edit: a small schema with every kind of help (hover
// docs, an enum for suggestions, a read-only field, a secret) and a document
// with a block to fold, a host problem and a secret placeholder.

export const SCHEMA = JSON.stringify({
  type: 'object',
  properties: {
    id: { type: 'string', readOnly: true, description: 'Assigned by the server' },
    listen: { type: 'integer', description: 'Port to listen on' },
    insecure_plaintext: { type: 'boolean', description: 'Serve without TLS' },
    mode: { enum: ['fast', 'safe'], description: 'Request handling' },
    server: {
      type: 'object',
      description: 'Where the app listens',
      properties: { host: { type: 'string' }, port: { type: 'integer' } },
    },
    token: { type: 'string', 'x-secret': true, description: 'API token' },
  },
});

export const VALUE = 'id: srv-42\nlisten: 7835\nmode: fast\nserver:\n  host: example.com\n  port: 80\ntoken: «secret 3f2a»\n';

export const HOST_DIAGNOSTICS = [{ path: '/server/host', message: 'Host unreachable', code: 'E_HOST' }];

export const COLORS = { key: '#ffcc66', bg: '#101418' };

export const SECRETS = { hint: 'Keeps the stored secret' };
