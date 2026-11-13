#!/usr/bin/env python3
"""Register a takt manager goal through the TUI, following the recorded procedure.

Why this exists: goal registration is only possible in the manager TUI, and the
dialog has traps that this script encodes so they are not re-learned each time:

- The dialog's initial selection is the "do not register" side
  (ManagerView.tsx: useRef(false)). Press UP once, then Enter.
- Any non-approval input while a proposal is pending withdraws the proposal
  (conversationService.ts). So while waiting for the dialog, send nothing else.
- Long goal text is delivered via a file on the mini (cat into terminal send),
  which is the delivery form that has proven reliable.

The script is attended automation: it performs the sequence and verifies the
result through takt-mcp (takt_list_goals). On any unexpected state it stops
and prints the terminal tail instead of guessing.

Usage:
  register_goal.py --goal-file goal.txt [--terminal HANDLE] [--timeout-min 15]
"""
import argparse, json, subprocess, sys, time

SSH_BASE = [
    "ssh",
    "-o", "ProxyCommand=python3 /home/hatch/workspace/.tailscale-ssh/proxy_connect.py %h %p",
    "-o", "UserKnownHostsFile=/home/hatch/.ssh/known_hosts_tailscale",
    "-i", "/home/hatch/.ssh/id_ed25519",
    "-o", "IdentitiesOnly=yes",
    "j5ik2o@100.106.166.1",
]
SCP_BASE = [
    "scp",
    "-o", "ProxyCommand=python3 /home/hatch/workspace/.tailscale-ssh/proxy_connect.py %h %p",
    "-o", "UserKnownHostsFile=/home/hatch/.ssh/known_hosts_tailscale",
    "-i", "/home/hatch/.ssh/id_ed25519",
    "-o", "IdentitiesOnly=yes",
]
DEFAULT_TERMINAL = "term_b7a6da7e-5f89-4b22-8f59-ed2898c308b1"  # fraktor takt manager
REPO = "/Users/j5ik2o/Sources/github.com/j5ik2o/fraktor-ng-rs"

# Minimal self-contained takt-mcp client used only for verification reads.
MCP_CLIENT = r'''
import json, subprocess, sys, threading, time
REPO = "/Users/j5ik2o/Sources/github.com/j5ik2o/fraktor-ng-rs"
proc = subprocess.Popen(["takt-mcp"], cwd=REPO, stdin=subprocess.PIPE,
                        stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
                        text=True, bufsize=1)
resp = {}
def reader():
    for line in proc.stdout:
        line = line.strip()
        if not line: continue
        try: msg = json.loads(line)
        except Exception: continue
        if "id" in msg: resp[msg["id"]] = msg
threading.Thread(target=reader, daemon=True).start()
def send(o):
    proc.stdin.write(json.dumps(o) + "\n"); proc.stdin.flush()
def wait(rid, timeout=60):
    t0 = time.time()
    while time.time() - t0 < timeout:
        if rid in resp: return resp[rid]
        time.sleep(0.2)
send({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
    "protocolVersion": "2024-11-05", "capabilities": {},
    "clientInfo": {"name": "goal-register-verify", "version": "0.1"}}})
wait(1)
send({"jsonrpc": "2.0", "method": "notifications/initialized"})
send({"jsonrpc": "2.0", "id": 2, "method": "tools/call",
      "params": {"name": "takt_list_goals", "arguments": {"cwd": REPO}}})
r = wait(2)
if r is None or "result" not in r:
    print("takt_list_goals failed", file=sys.stderr)
    sys.exit(2)
text = "".join(c.get("text", "") for c in r["result"]["content"] if c.get("type") == "text")
for g in json.loads(text).get("goals", []):
    print(g["id"])
proc.kill()
'''


def ssh(cmd, timeout=60, check=False):
    r = subprocess.run(SSH_BASE + [cmd], capture_output=True, text=True, timeout=timeout)
    if check and r.returncode != 0:
        raise SystemExit(f"command failed ({r.returncode}): {cmd}\n{r.stderr[-500:]}")
    return r.stdout


def log(msg):
    print(f"[{time.strftime('%H:%M:%S')}] {msg}", flush=True)


def terminal_tail(terminal, limit=30):
    out = ssh(f"orca terminal read --terminal {terminal} --limit {limit}")
    return out


_client_uploaded = False


def list_goal_ids():
    global _client_uploaded
    if not _client_uploaded:
        import tempfile, os
        fd, path = tempfile.mkstemp(suffix=".py")
        with os.fdopen(fd, "w") as f:
            f.write(MCP_CLIENT)
        subprocess.run(SCP_BASE + [path, "j5ik2o@100.106.166.1:/tmp/_goal_ids.py"], check=True)
        os.unlink(path)
        _client_uploaded = True
    out = ssh('export PATH="$HOME/.local/share/mise/shims:$PATH"; python3 /tmp/_goal_ids.py', timeout=120, check=True)
    ids = {line.strip() for line in out.splitlines() if line.strip()}
    if not ids:
        raise SystemExit("goal listing returned no goals; treating as failure, not as an empty registry")
    return ids


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--goal-file", required=True)
    ap.add_argument("--terminal", default=DEFAULT_TERMINAL)
    ap.add_argument("--timeout-min", type=int, default=15)
    args = ap.parse_args()

    log("baseline: reading current goals via takt-mcp")
    before = list_goal_ids()
    log(f"baseline goals: {sorted(before)}")

    tail = terminal_tail(args.terminal)
    if "登録せずに" in tail and "登録する" in tail:
        log("ABORT: a registration dialog already appears to be open. Handle it first.")
        print(tail[-1500:])
        return 1

    log("sending goal text via file")
    subprocess.run(SCP_BASE + [args.goal_file, "j5ik2o@100.106.166.1:/tmp/goal-register.txt"], check=True)
    ssh('orca terminal send --terminal %s --text "$(cat /tmp/goal-register.txt)" --enter --wait-submit 30'
        % args.terminal, timeout=90)

    log("waiting for the registration dialog (sending nothing else meanwhile)")
    deadline = time.time() + args.timeout_min * 60
    dialog_seen = False
    while time.time() < deadline:
        time.sleep(10)
        tail = terminal_tail(args.terminal)
        # The proposal dialog lists both choices; the negative one is distinctive.
        if "登録せずに" in tail:
            dialog_seen = True
            break
    if not dialog_seen:
        log("TIMEOUT: no registration dialog appeared. Terminal tail:")
        print(tail[-2000:])
        return 2

    log("dialog detected: UP once, then Enter (recorded procedure)")
    ssh('orca terminal send --terminal %s --text "$(printf "\\033[A")"' % args.terminal, timeout=60)
    time.sleep(1)
    ssh('orca terminal send --terminal %s --text "$(printf "\\r")"' % args.terminal, timeout=60)

    log("verifying registration via takt-mcp")
    for _ in range(12):
        time.sleep(5)
        after = list_goal_ids()
        new = after - before
        if new:
            log(f"REGISTERED: new goal id(s): {sorted(new)}")
            return 0
    log("UNVERIFIED: dialog was confirmed but no new goal appeared. Terminal tail:")
    print(terminal_tail(args.terminal)[-2000:])
    return 3


if __name__ == "__main__":
    sys.exit(main())
