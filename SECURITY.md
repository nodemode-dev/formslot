# Security policy

## Supported versions

Security fixes go into the newest release. Please update before reporting, and check whether the problem is still there.

| Version | Supported |
| --- | --- |
| Latest release | Yes |
| Anything older | No |

## Reporting a vulnerability

**Do not open a public issue for a security problem.** Use GitHub's private reporting instead: on the repository's **Security** tab, choose **Report a vulnerability**. Include what you found, the steps to reproduce it, the version and platform, and what someone could do with it.

You can expect an acknowledgement within a week. We will keep you informed while it is fixed and credit you in the release notes, unless you would rather not be named.

## What formslot guards

formslot runs on your own computer and has no window. The things that matter most are the ways something outside it could reach your code or your key.

- **The local service.** `formslot serve` listens on `127.0.0.1` only, port 5645 unless `FORMSLOT_PORT` says otherwise. Every request is checked three ways before it reaches a route:
  - the `Host` header must be `127.0.0.1` or `localhost` with the service's own port, or the answer is 421;
  - an `Origin` header, when one is sent, must be this computer on that port, or the answer is 403, so a web page on another site cannot drive the service;
  - the `Authorization: Bearer` token must match, compared in constant time, or the answer is 401.

  The token is 64 hex characters, made new on every start. Bodies are limited to 1 MiB and chunked bodies are refused.
- **The discovery file.** The token is written to `service.json` in the data folder so the command line and the hooks can find the service. On macOS and Linux that file is created with mode 0600 and the data folder with mode 0700, so only your user can read them. On Windows the file takes the access rights of your user folder; formslot sets no extra rights there.
- **What reaches an agent.** Code leaves the index only inside an envelope that says it is data, with its source, licence and copy verdict. A file with a secret finding is withheld, and every answer passes a second redaction before it leaves. Agents read owner rules and cannot change them; an agent can only propose a record.
- **Your Anthropic key.** `formslot key set` saves it in the system password store, or in a file only you can read where the computer has none. `formslot key status` never reads the password store. No command prints the key, and it is used by one command only, `formslot pack --count remote`.
- **What leaves your computer.** Nothing, unless you run one of three commands yourself: `pack --count remote`, `dense fetch --yes` or `eval --arm model`. [`docs/GUIDE.md`](docs/GUIDE.md) says what each one sends and to whom. There is no telemetry, no account and no update check.
- **Hooks fail open.** The check before a write exits 0 when formslot itself fails, so a broken check never blocks your work. It also means a failed check is not a guard: `formslot doctor` reports whether the hooks can find formslot.

Anything that makes the service reachable from another computer, lets another origin or another user read the token, gets a secret past the redaction, or lets an agent change an owner rule counts as a vulnerability.

## What does not count

- A program already running as your user reading files in your data folder. It can read your code too.
- Anything that needs you to paste a command an attacker wrote.
- The content of repositories you chose to index reaching the agent you connected.
