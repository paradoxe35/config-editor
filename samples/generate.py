#!/usr/bin/env python3
"""Generates the large demo config in every supported language.

One deterministic data model (`config()`) is rendered as JSON, YAML, TOML,
HCL, and XML. Every file must parse to the same value and validate against
`schema.json`; the test `tests/demo_samples.rs` enforces both.

Run from the repository root:

    python3 samples/generate.py

XML constraints: repeated sibling elements form an array and a single one reads
as a scalar, so every array holds at least two items (asserted). Map keys must
be valid XML element names, so path-keyed maps (e.g. a site's `redirects`) are
left out.
"""

import json
import re
from pathlib import Path

OUT = Path(__file__).resolve().parent

SITES = [
    "etiolles", "hetzner", "ikoula", "scaleway", "ovh", "gravelines",
    "roubaix", "strasbourg", "frankfurt", "helsinki", "amsterdam", "london",
]
ROLES = ["pve", "pve-ai", "pve-stagging", "pve-prod", "ai-server", "nas", "edge", "backup"]
PEOPLE = [
    "paradoxe-pc", "paradoxe-laptop", "gaby-pc", "gaby-phone", "ops-oncall",
    "ci-runner", "grafana-bot", "backup-agent", "alice-laptop", "bob-desktop",
    "carol-tablet", "dave-workstation", "erin-laptop", "frank-pc", "grace-phone",
    "heidi-laptop", "ivan-desktop", "judy-laptop", "mallory-audit", "niaj-laptop",
    "olivia-pc", "peggy-laptop", "rupert-desktop", "sybil-laptop", "trent-admin",
]
APPS = [
    ("whoami", 8000), ("speedtest", 8080), ("grafana", 3000), ("prometheus", 9090),
    ("loki", 3100), ("gitea", 3000), ("drone", 8000), ("nextcloud", 80),
    ("vaultwarden", 8081), ("jellyfin", 8096), ("immich", 2283), ("paperless", 8000),
    ("homeassistant", 8123), ("uptime", 3001), ("minio", 9001), ("keycloak", 8443),
    ("outline", 3000), ("plausible", 8000), ("umami", 3000), ("n8n", 5678),
    ("portainer", 9443), ("traefik", 8082), ("pihole", 80), ("adguard", 3000),
    ("wikijs", 3000), ("bookstack", 80), ("mealie", 9000), ("firefly", 8080),
    ("photoprism", 2342), ("syncthing", 8384),
]
DOMAINS = ["gracecarry.shop", "pngwasi.me", "example.dev"]
MODES = ["failover", "balance", "least_connections", "ip_hash"]


def fingerprint(*parts):
    """A stable 8-hex-digit fingerprint, shown as `«secret xxxxxxxx»`."""
    value = 2166136261
    for character in "/".join(parts):
        value = ((value ^ ord(character)) * 16777619) & 0xFFFFFFFF
    return f"{value:08x}"


def secret(*parts):
    return f"«secret {fingerprint(*parts)}»"


def config():
    clients = {}
    for site in SITES:
        for role in ROLES:
            name = f"{site}-{role}"
            entry = {"secret": secret("client", name)}
            index = len(clients)
            if index % 3 == 0:
                # Rotation: the old and the new secret are both accepted.
                entry["secret"] = [secret("client", name), secret("client", name, "next")]
            if index % 4 == 1:
                entry["max_connections"] = 256 + index * 8
            clients[name] = entry
    client_names = list(clients)

    visitors = {}
    for index, person in enumerate(PEOPLE):
        entry = {"secret": secret("visitor", person)}
        if index % 5 == 0:
            entry["admin"] = "write"
        elif index % 5 == 1:
            entry["admin"] = "read"
        if index % 3 == 0:
            entry["reach"] = {}
        elif index % 3 == 1:
            entry["reach"] = {
                client_names[index % len(client_names)]: [
                    f"{SITES[index % len(SITES)]}-pve-ssh",
                    f"{SITES[index % len(SITES)]}-pve-web",
                ],
                client_names[(index * 7) % len(client_names)]: "self",
            }
        if index % 4 == 2:
            entry["max_connections"] = 32 + index
        if index % 6 == 3:
            entry["admin_from"] = [f"10.{index}.0.0/16", f"192.168.{index}.0/24"]
        visitors[person] = entry
    visitor_names = list(visitors)

    services = {}
    port = 65000
    for site_index, site in enumerate(SITES):
        pve = f"{site}-pve"
        ai = f"{site}-pve-ai"
        services[f"{site}-pve-ssh"] = {
            "client": pve,
            "target": 22,
            "visitors": [visitor_names[site_index % 5], visitor_names[(site_index + 1) % 5]],
        }
        services[f"{site}-pve-web"] = {
            "target": 8006,
            "clients": [pve, ai],
            "mode": MODES[site_index % len(MODES)],
            "visitors": [visitor_names[0], visitor_names[(site_index + 2) % len(visitor_names)]],
            "health_check": {
                "interval_secs": 10 + site_index,
                "timeout_secs": 3,
                "failures": 2,
                "path": "/api2/json/version",
            },
        }
        port += 1
        services[f"{site}-vm-ssh"] = {
            "bind": port,
            "target": f"192.168.{50 + site_index}.142:22",
            "clients": [pve, ai],
            "allow_from": [f"10.{site_index}.0.0/16", "192.168.0.0/16"],
            "deny_from": ["10.66.0.0/16", "172.16.0.0/12"],
            "max_connections": 64,
        }
        port += 1
        services[f"{site}-wireguard"] = {
            "target": f"192.168.{50 + site_index}.164:51820",
            "bind": port,
            "protocol": "udp",
            "clients": [pve, ai],
            "visitors": [visitor_names[0], visitor_names[1]],
            "bandwidth": f"{10 * (site_index + 1)}mbit",
        }
        services[f"{site}-wgdashboard"] = {
            "target": f"192.168.{50 + site_index}.164:10086",
            "clients": [
                {"name": pve, "target": 10086, "weight": 10 + site_index},
                {"name": ai, "target": f"192.168.{50 + site_index}.165:10086", "weight": 1},
            ],
            "mode": "balance",
            "visitors": [visitor_names[0], visitor_names[2]],
            "proxy_protocol": "v2" if site_index % 2 else "v1",
        }
        services[f"{site}-backup-sync"] = {
            "client": f"{site}-backup",
            "target": f"192.168.{50 + site_index}.20:873",
            "health_check": site_index % 2 == 0,
            "visitors": [visitor_names[7], visitor_names[8]],
        }

    for app_index, (app, app_port) in enumerate(APPS):
        domain = DOMAINS[app_index % len(DOMAINS)]
        site = SITES[app_index % len(SITES)]
        host = f"{app}.{domain}"
        website = {
            "hosts": [host, f"www.{host}"],
            "tls": ["auto", "off", "internal", "auto"][app_index % 4],
            "compress": app_index % 2 == 0,
            "hsts": app_index % 3 != 1,
            "request_headers": {
                "X-Forwarded-Proto": "https",
                "X-Burrow-Service": app,
            },
            "response_headers": {
                "X-Frame-Options": "SAMEORIGIN",
                "Referrer-Policy": "strict-origin-when-cross-origin",
            },
        }
        if app_index % 3 == 0:
            website["paths"] = ["/", f"/{app}/api"]
            website["redirect_from"] = [f"old-{app}.{domain}", f"{app}.legacy.{domain}"]
        if app_index % 4 == 1:
            website["basic_auth"] = {
                "admin": f"$2y$05${fingerprint('basic', app)}admin",
                "viewer": f"$2y$05${fingerprint('basic', app, 'viewer')}viewer",
            }
        if app_index % 5 == 2:
            website["forward_auth"] = "sso"
            website["rate_limit"] = 50 + app_index
        if app_index % 6 == 3:
            website["target_tls"] = {"insecure_skip_verify": True}
        elif app_index % 6 == 4:
            website["target_tls"] = True
            website["target_http2"] = True
        if app_index % 7 == 5:
            website["maintenance"] = False
            website["max_body_size"] = "64MB"
            website["response_header_timeout_secs"] = 120
        if app_index % 9 == 7:
            website["client_auth"] = {"ca": f"certs/{app}-clients-ca.pem", "crl": f"certs/{app}.crl"}
        entry = {
            "client": f"{site}-pve-prod",
            "target": f"192.168.0.{11 + app_index}:{app_port}",
            "web": website,
        }
        if app_index % 4 == 3:
            del entry["client"]
            entry["clients"] = [f"{site}-pve-prod", f"{site}-edge"]
            entry["mode"] = "failover"
        services[app] = entry

    # Short web forms: a single host name and a host list.
    services["status-page"] = {
        "client": "hetzner-pve-prod",
        "target": "192.168.0.90:80",
        "web": "status.gracecarry.shop",
    }
    services["docs-site"] = {
        "client": "hetzner-pve-prod",
        "target": "192.168.0.91:80",
        "web": ["docs.gracecarry.shop", "docs.pngwasi.me"],
    }
    services["self-metrics"] = {"client": "self", "target": "127.0.0.1:9100"}

    web = {
        "http": "127.0.0.1:8080",
        "https": 443,
        "https_redirect": True,
        "http2": True,
        "rate_limit": 200,
        "request_header_timeout_secs": 30,
        "idle_timeout_secs": 300,
        "request_id": True,
        "hsts": True,
        "compress": True,
        "max_body_size": "32MB",
        "response_header_timeout_secs": 60,
        "error_pages": "errors/",
        "trusted_proxies": ["10.0.0.0/8", "192.168.0.0/16"],
        "proxy_protocol_from": ["10.10.0.1", "10.10.0.2"],
        "access_log": {"file": "logs/access.log", "format": "json"},
        "forward_auth": {
            "sso": {
                "service": "keycloak",
                "path": "/realms/burrow/protocol/openid-connect/auth",
                "copy_headers": ["X-Auth-User", "X-Auth-Email"],
            },
            "basic": {"service": "outline", "path": "/auth/verify", "copy_headers": ["X-User", "X-Groups"]},
        },
        "redirect_names": {
            "old.gracecarry.shop": "gracecarry.shop",
            "legacy.pngwasi.me": {"to": "pngwasi.me", "permanent": True},
            "beta.example.dev": {"to": "example.dev", "permanent": False},
        },
        "request_headers": {"X-Served-By": "burrow", "X-Region": "eu-west"},
        "response_headers": {"Server": "burrow", "X-Content-Type-Options": "nosniff"},
        "certificates": [
            {
                "hosts": ["internal.gracecarry.shop", "intranet.gracecarry.shop"],
                "cert": "certs/internal/fullchain.pem",
                "key": "certs/internal/privkey.pem",
            },
            {"hosts": ["vpn.pngwasi.me", "wg.pngwasi.me"], "upload": "vpn-cert"},
        ],
        "letsencrypt": {
            "email": "paradoxngwasi@outlook.com",
            "directory": "production",
            "profile": "tlsserver",
            "dns": [
                {
                    "domains": ["gracecarry.shop", "pngwasi.me"],
                    "wildcards": ["*.gracecarry.shop", "*.pngwasi.me"],
                    "cloudflare": {"token": "${CLOUDFLARE_API_TOKEN}"},
                    "wait_secs": 60,
                    "resolvers": ["1.1.1.1:53", "1.0.0.1:53"],
                },
                {
                    "domains": ["example.dev", "lab.example.dev"],
                    "rfc2136": {
                        "server": "ns1.example.dev:53",
                        "key_name": "burrow-acme",
                        "secret": "${RFC2136_SECRET}",
                        "algorithm": "hmac-sha256",
                    },
                    "wait_secs": 120,
                },
                {
                    "domains": ["acme.pngwasi.me", "edge.pngwasi.me"],
                    "acme_dns": {
                        "api": "https://auth.acme-dns.io",
                        "username": "burrow",
                        "password": "${ACME_DNS_PASSWORD}",
                        "subdomain": "d420c923-bbd7-4056-ab64-c3ca54c9b3cf",
                    },
                },
            ],
        },
    }

    return {
        "listen": "0.0.0.0:7835",
        "public_address": "tunnel.pngwasi.me",
        "insecure_plaintext": False,
        "update": {"remote": True},
        "websocket": {
            "listen": "127.0.0.1:7836",
            "path": "/burrow",
            "host": "tunnel.pngwasi.me",
            "keepalive_secs": 20,
            "challenge": "http",
        },
        "heartbeat": {"interval_secs": 15},
        "limits": {
            "handshake_timeout_secs": 5,
            "max_handshakes": 4096,
            "max_connections_per_client": 8192,
            "max_pending": 512,
        },
        "pool": {"min_idle": 4, "max_idle": 64},
        "tcp": {"nodelay": True, "keepalive_secs": 30},
        "udp": {"session_timeout_secs": 120, "max_peers": 4096},
        "metrics": {"listen": "127.0.0.1:9464"},
        "self": {"allow": ["127.0.0.1/32", "10.0.0.0/8"]},
        "clients": clients,
        "visitors": visitors,
        "services": services,
        "web": web,
    }


# ---------------------------------------------------------------------------
# Renderers
# ---------------------------------------------------------------------------

def check_arrays(value, path="$"):
    if isinstance(value, list):
        assert len(value) >= 2, f"{path}: XML needs two or more items per array"
        for index, item in enumerate(value):
            check_arrays(item, f"{path}[{index}]")
    elif isinstance(value, dict):
        for key, item in value.items():
            check_arrays(item, f"{path}.{key}")


def quoted(text):
    return json.dumps(text, ensure_ascii=False)


# YAML --------------------------------------------------------------------

YAML_PLAIN = re.compile(r"^[A-Za-z/][A-Za-z0-9_./@-]*$")
YAML_RESERVED = {"true", "false", "yes", "no", "on", "off", "null", "y", "n", "~"}


def yaml_scalar(value):
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, (int, float)):
        return str(value)
    if YAML_PLAIN.match(value) and value.lower() not in YAML_RESERVED:
        return value
    return quoted(value)


def yaml_key(key):
    return key if YAML_PLAIN.match(key) and key.lower() not in YAML_RESERVED else quoted(key)


def yaml_lines(value, indent):
    pad = "  " * indent
    lines = []
    for key, item in value.items():
        if isinstance(item, dict):
            if not item:
                lines.append(f"{pad}{yaml_key(key)}: {{}}")
            else:
                lines.append(f"{pad}{yaml_key(key)}:")
                lines.extend(yaml_lines(item, indent + 1))
        elif isinstance(item, list):
            lines.append(f"{pad}{yaml_key(key)}:")
            for element in item:
                if isinstance(element, dict):
                    nested = yaml_lines(element, indent + 2)
                    first = nested[0].lstrip()
                    lines.append(f"{pad}  - {first}")
                    lines.extend(nested[1:])
                else:
                    lines.append(f"{pad}  - {yaml_scalar(element)}")
        else:
            lines.append(f"{pad}{yaml_key(key)}: {yaml_scalar(item)}")
    return lines


def to_yaml(data):
    header = "# burrow server configuration (large demo); secrets show as fingerprints or ${ENV} names\n"
    return header + "\n".join(yaml_lines(data, 0)) + "\n"


# TOML --------------------------------------------------------------------

TOML_BARE = re.compile(r"^[A-Za-z0-9_-]+$")


def toml_key(key):
    return key if TOML_BARE.match(key) else quoted(key)


def toml_inline(value):
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, (int, float)):
        return str(value)
    if isinstance(value, str):
        return quoted(value)
    if isinstance(value, list):
        return "[" + ", ".join(toml_inline(item) for item in value) + "]"
    if not value:
        return "{}"
    return "{ " + ", ".join(f"{toml_key(k)} = {toml_inline(v)}" for k, v in value.items()) + " }"


def is_table(value):
    return isinstance(value, dict) and bool(value)


def is_table_array(value):
    return isinstance(value, list) and value and all(isinstance(item, dict) for item in value)


def toml_body(value, path, out, header):
    scalars = [(k, v) for k, v in value.items() if not is_table(v) and not is_table_array(v)]
    tables = [(k, v) for k, v in value.items() if is_table(v) or is_table_array(v)]
    if header and (scalars or not tables):
        out.append("")
        out.append(header)
    for key, item in scalars:
        out.append(f"{toml_key(key)} = {toml_inline(item)}")
    for key, item in tables:
        child = path + [toml_key(key)]
        if is_table_array(item):
            for element in item:
                toml_body(element, child, out, f"[[{'.'.join(child)}]]")
        else:
            toml_body(item, child, out, f"[{'.'.join(child)}]")


def to_toml(data):
    out = ["# burrow server configuration (large demo); secrets show as fingerprints or ${ENV} names"]
    toml_body(data, [], out, None)
    return "\n".join(out) + "\n"


# HCL ---------------------------------------------------------------------

HCL_IDENT = re.compile(r"^[A-Za-z_][A-Za-z0-9_-]*$")


def hcl_key(key):
    return key if HCL_IDENT.match(key) else quoted(key)


def hcl_string(text):
    # `${` and `%{` start template interpolation in HCL strings: escape them.
    return quoted(text).replace("${", "$${").replace("%{", "%%{")


def hcl_expr(value, indent):
    pad = "  " * indent
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, (int, float)):
        return str(value)
    if isinstance(value, str):
        return hcl_string(value)
    if isinstance(value, list):
        if all(not isinstance(item, (dict, list)) for item in value):
            return "[" + ", ".join(hcl_expr(item, indent) for item in value) + "]"
        items = [f"{pad}  {hcl_expr(item, indent + 1)}," for item in value]
        return "[\n" + "\n".join(items) + f"\n{pad}]"
    if not value:
        return "{}"
    items = [f"{pad}  {hcl_key(k)} = {hcl_expr(v, indent + 1)}" for k, v in value.items()]
    return "{\n" + "\n".join(items) + f"\n{pad}}}"


def hcl_body(value, indent):
    pad = "  " * indent
    lines = []
    for key, item in value.items():
        blockable = (
            isinstance(item, dict)
            and item
            and HCL_IDENT.match(key)
            and all(HCL_IDENT.match(k) for k in item)
        )
        if blockable:
            lines.append(f"{pad}{key} {{")
            lines.extend(hcl_body(item, indent + 1))
            lines.append(f"{pad}}}")
        else:
            lines.append(f"{pad}{hcl_key(key)} = {hcl_expr(item, indent)}")
    return lines


def to_hcl(data):
    header = "# burrow server configuration (large demo); secrets show as fingerprints or ${ENV} names\n"
    return header + "\n".join(hcl_body(data, 0)) + "\n"


# XML ---------------------------------------------------------------------

def xml_text(value):
    if isinstance(value, bool):
        return "true" if value else "false"
    return str(value).replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")


def xml_lines(key, value, indent):
    pad = "  " * indent
    if isinstance(value, list):
        lines = []
        for item in value:
            lines.extend(xml_lines(key, item, indent))
        return lines
    if isinstance(value, dict):
        if not value:
            return [f"{pad}<{key}/>"]
        lines = [f"{pad}<{key}>"]
        for child, item in value.items():
            lines.extend(xml_lines(child, item, indent + 1))
        lines.append(f"{pad}</{key}>")
        return lines
    return [f"{pad}<{key}>{xml_text(value)}</{key}>"]


def to_xml(data):
    lines = [
        '<?xml version="1.0" encoding="UTF-8"?>',
        "<!-- burrow server configuration (large demo); secrets show as fingerprints or ${ENV} names -->",
        '<burrow config-root="true">',
    ]
    for key, value in data.items():
        lines.extend(xml_lines(key, value, 1))
    lines.append("</burrow>")
    return "\n".join(lines) + "\n"


def main():
    data = config()
    check_arrays(data)
    outputs = {
        "burrow.large.json": json.dumps(data, indent=2, ensure_ascii=False) + "\n",
        "burrow.large.yaml": to_yaml(data),
        "burrow.large.toml": to_toml(data),
        "burrow.large.hcl": to_hcl(data),
        "burrow.large.xml": to_xml(data),
    }
    for name, text in outputs.items():
        (OUT / name).write_text(text, encoding="utf-8")
        print(f"{name}: {text.count(chr(10))} lines, {len(text.encode('utf-8'))} bytes")


if __name__ == "__main__":
    main()
