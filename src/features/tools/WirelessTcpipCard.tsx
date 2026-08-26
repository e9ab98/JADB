import { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import '@/i18n';
import {
  AlertTriangle,
  Check,
  ChevronDown,
  ChevronUp,
  Clipboard,
  Keyboard,
  Loader2,
  PlugZap,
  RefreshCw,
  Wifi,
} from 'lucide-react';
import { toast } from 'sonner';
import { Card, CardContent } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Badge } from '@/components/ui/badge';
import { adbConnect, adbReconnect, adbShell, adbTcpip } from '@/ipc/adb';

/**
 * "Open TCP/IP port on the device" tool, v3.
 *
 * Improvements over v2 (which broke on devices whose `ip` command
 * returns empty -- common when the device isn't actually on WiFi
 * or when the kernel routing table has no IPv4 default route):
 *
 *   - IP probe starts with the unfiltered `ip -4 addr show`
 *     (returns every iface at once -- critical for OEM-renamed
 *     interfaces that aren't in our hardcoded list), then per-iface
 *     `ip -4 addr show <iface>` for the common names, then
 *     `ip route get 1.1.1.1` for the kernel view, then
 *     `dumpsys wifi` for the privileged `mIpAddress`, then
 *     `ifconfig` and `getprop dhcp.*` as last resorts. Every
 *     probe's raw stdout/stderr is preserved so the "raw output"
 *     panel shows exactly what each command returned -- which is
 *     the only way to diagnose a no-IP-found failure without
 *     rerunning with a debug shell.
 *   - When all probes come up empty, the user gets a "type the IP
 *     manually" input as a last-resort path -- useful when the
 *     device is on a network where no shell probe can reach the
 *     right interface (VPN-only, IPv6-only, etc.).
 *
 * Pipeline (unchanged shape):
 *   1. `adb -s <serial> tcpip <port>` with port fallback (5555
 *      -> 5556 -> 5557 -> 5558).
 *   2. Probe every reasonable IPv4 source; surface all hits.
 *   3. Each candidate IP gets its own row with copy + "connect
 *      from this app" buttons.
 *   4. Bottom: `reconnect` button drops & re-establishes the
 *      current serial.
 */
type State =
  | { kind: 'idle' }
  | { kind: 'running' }
  | { kind: 'ok'; ip: string; port: number; tcpipOut: string; candidates: IpCandidate[]; probes: ProbeResult[] }
  | { kind: 'error'; message: string; probes: ProbeResult[]; candidates?: IpCandidate[] };

type IpSource = 'route' | 'iface' | 'prop' | 'dumpsys' | 'manual';
type IpCandidate = { ip: string; source: IpSource; iface?: string; detail?: string };
type ProbeSummary = {
  /** Human-readable "ip-all", "ip-iface::wlan0", etc. */
  source: string;
  /** Did the probe yield a usable IPv4? */
  status: 'hit' | 'empty' | 'threw';
  /** IPv4 we extracted, if any. */
  ip?: string;
  /** Iface name when the probe was per-iface. */
  iface?: string | undefined;
  /** Raw stdout/stderr (kept around for the "raw output" panel). */
  raw: string;
};
type ProbeResult = ProbeSummary;

// Ports we'll auto-fall-back to if the user-picked one is busy.
const FALLBACK_PORTS = [5555, 5556, 5557, 5558];

// `ifconfig` (BSD-style) `inet X.X.X.X ...` or `inet addr:X.X.X.X ...`.
const IFCONFIG_RE = /inet(?:\s+addr:)?\s+(\d+\.\d+\.\d+\.\d+)/g;
// `getprop` value that looks like a bare IPv4 (no CIDR, no whitespace).
const BARE_IPV4_RE = /^(\d+\.\d+\.\d+\.\d+)$/;

async function runProbe(
  serial: string,
  label: string,
  cmd: string,
  iface?: string,
): Promise<ProbeResult> {
  // Wrap every probe so the user can see exactly which command
  // fired and what came back. The status field is filled by the
  // caller after parsing -- runProbe just records the raw output.
  try {
    const out = await adbShell(serial, cmd);
    const raw = (out?.stdout ?? '') + (out?.stderr ? `\n[stderr]\n${out.stderr}` : '');
    return {
      source: `${label} :: ${cmd}`,
      status: 'empty', // overwritten by caller after parsing
      iface,
      raw: raw.trim() || '<empty>',
    };
  } catch (e) {
    return { source: `${label} :: ${cmd}`, status: 'threw', iface, raw: `<threw> ${String(e)}` };
  }
}

async function collectCandidateIps(serial: string): Promise<{
  candidates: IpCandidate[];
  probes: ProbeResult[];
}> {
  // Dedupe by IP; preserve insertion order so the most-trustworthy
  // sources win on tie.
  const candidates: IpCandidate[] = [];
  const seen = new Set<string>();
  const probes: ProbeResult[] = [];

  function push(ip: string | null | undefined, source: IpSource, iface?: string, detail?: string) {
    if (!ip) return;
    if (seen.has(ip)) return;
    seen.add(ip);
    const cand: IpCandidate = iface ? { ip, source, iface } : { ip, source };
    if (detail) cand.detail = detail;
    candidates.push(cand);
  }

  // 1) `ip -4 addr show` (no iface filter). This is the
  // broadest probe: returns every IPv4 iface on the device.
  // Critical for OEM-renamed ifaces (some Huawei / Honor
  // devices use `wlan2` or `wlan-a0`; some HyperOS builds use
  // `p2p-wlan0-0`) which our hardcoded list below doesn't
  // cover. We scan every `inet ` line and pair it with the
  // iface name that appears on the immediately preceding
  // `N: <iface>: <...>` line.
  const ipAllProbe = await runProbe(serial, 'ip-all', 'ip -4 addr show');
  if (ipAllProbe.raw !== '<empty>' && !ipAllProbe.raw.startsWith('<threw')) {
    let curIface: string | undefined;
    let hitIps: string[] = [];
    for (const line of ipAllProbe.raw.split(/\r?\n/)) {
      const ifaceMatch = /^\s*\d+:\s+(\S+):\s/.exec(line);
      if (ifaceMatch && ifaceMatch[1]) {
        curIface = ifaceMatch[1];
        continue;
      }
      const inetMatch = /^\s+inet\s+(\d+\.\d+\.\d+\.\d+)/.exec(line);
      if (inetMatch && inetMatch[1] && !inetMatch[1].startsWith('127.')) {
        hitIps.push(inetMatch[1]);
        push(inetMatch[1], 'iface', curIface);
      }
    }
    if (hitIps.length > 0) {
      ipAllProbe.status = 'hit';
      ipAllProbe.ip = hitIps[0] ?? '';
    }
  }
  probes.push(ipAllProbe);

  // 2) `ip -4 addr show <iface>` for the most common wireless /
  // wired names. Same call shape as JADB's `system_info` (no
  // `-o`, no fancy flags). This is the belt-and-suspenders
  // probe: even if the unfiltered form above didn't surface
  // the iface (some kernels hide DOWN ifaces from the list),
  // asking for the iface by name often still works.
  for (const iface of ['wlan0', 'eth0', 'wlan1', 'eth1', 'wlan2', 'p2p0', 'rndis0', 'usb0']) {
    const probe = await runProbe(serial, 'ip-iface', `ip -4 addr show ${iface}`, iface);
    // `ip -4 addr show` outputs (toybox-style):
    //   "3: wlan0: <BROADCAST,MULTICAST,UP,LOWER_UP> mtu 1500 ...
    //    inet 192.168.1.100/24 brd 192.168.1.255 scope global wlan0"
    // We look for `inet <ip>` and capture the IP. The CIDR suffix
    // is dropped by `split_whitespace`. Note the absence of `inet`
    // can mean either "iface doesn't exist" (toybox returns
    // "Device does not exist." on stderr) or "iface exists but
    // has no IPv4" (empty body). The two are indistinguishable
    // from the probe alone, which is why we record 'empty' even
    // on stderr-bearing responses.
    const idx = probe.raw.indexOf('inet ');
    if (idx >= 0) {
      const ip = probe.raw.slice(idx + 5).split(/\s+/)[0] ?? '';
      const cleaned = ip.split('/')[0] ?? '';
      if (BARE_IPV4_RE.test(cleaned) && !cleaned.startsWith('127.')) {
        probe.status = 'hit';
        probe.ip = cleaned;
        push(cleaned, 'iface', iface);
      }
    }
    probes.push(probe);
  }

  // 3) `ip route get` -- kernel routing view. May be a VPN
  // address on devices with split-tunneling.
  const routeProbe = await runProbe(serial, 'ip-route', 'ip route get 1.1.1.1');
  const routeSrc = /\bsrc\s+(\d+\.\d+\.\d+\.\d+)/.exec(routeProbe.raw);
  if (routeSrc && routeSrc[1]) {
    routeProbe.status = 'hit';
    routeProbe.ip = routeSrc[1];
    push(routeSrc[1], 'route');
  }
  probes.push(routeProbe);

  // 4) `dumpsys wifi` -- the same source JADB's `system_info`
  // uses for `wifi_ip` on Android 13+ (privileged --realtime
  // view). Output is verbose; the field name varies by OEM:
  //   AOSP / Pixel       : `mIpAddress: X.X.X.X`
  //   Xiaomi / HyperOS   : `mIpaddr=X.X.X.X` (no colon, equals sign)
  //   older AOSP         : `IpAddress: X.X.X.X`
  //   rare               : bare `inet X.X.X.X` line
  // We try each in turn; the first hit wins.
  const wifiProbe = await runProbe(serial, 'dumpsys-wifi', 'dumpsys wifi 2>/dev/null');
  const WIFI_PATTERNS: RegExp[] = [
    /mIpAddress\s*[:=]\s*"?(\d+\.\d+\.\d+\.\d+)"?/,
    /mIpaddr\s*[:=]\s*"?(\d+\.\d+\.\d+\.\d+)"?/,
    /\bIpAddress\s*[:=]\s*"?(\d+\.\d+\.\d+\.\d+)"?/,
    /\bipaddr\s*[:=]\s*"?(\d+\.\d+\.\d+\.\d+)"?/,
  ];
  for (const re of WIFI_PATTERNS) {
    const m = re.exec(wifiProbe.raw);
    if (m && m[1] && !m[1].startsWith('127.')) {
      wifiProbe.status = 'hit';
      wifiProbe.ip = m[1];
      push(m[1], 'dumpsys', 'wlan0', 'dumpsys wifi');
      break;
    }
  }
  probes.push(wifiProbe);

  // 5) `ifconfig` -- last-resort shell tool when `ip` is missing
  // entirely (very old or heavily-stripped ROMs). The output
  // shape is BSD-style `inet addr:X.X.X.X`; we drop loopback.
  for (const cmd of ['ifconfig 2>/dev/null', 'toolbox ifconfig 2>/dev/null']) {
    const probe = await runProbe(serial, 'ifconfig', cmd);
    const ips: string[] = [];
    for (const m of probe.raw.matchAll(IFCONFIG_RE)) {
      const ip = m[1];
      if (!ip) continue;
      if (ip.startsWith('127.')) continue;
      ips.push(ip);
      push(ip, 'iface');
    }
    if (ips.length > 0) {
      probe.status = 'hit';
      probe.ip = ips[0] ?? '';
    }
    probes.push(probe);
  }

  // 6) Legacy DHCP-set props. Cheap, sometimes the only thing
  // that works on a freshly-booted device whose IP allocator
  // hasn't populated the routing table yet.
  for (const key of ['dhcp.wlan0.ipaddress', 'dhcp.eth0.ipaddress']) {
    const probe = await runProbe(serial, 'getprop', `getprop ${key}`, key);
    const m = BARE_IPV4_RE.exec(probe.raw.trim().split(/\s+/)[0] ?? '');
    if (m && m[1]) {
      probe.status = 'hit';
      probe.ip = m[1];
      push(m[1], 'prop', key);
    }
    probes.push(probe);
  }

  return { candidates, probes };
}

async function tryTcpip(serial: string, port: number): Promise<string> {
  // Returns raw `adb` stdout on success; throws on failure with
  // the raw stderr wrapped in an Error.
  const out = await adbTcpip(serial, port);
  return out.trim();
}

export function WirelessTcpipCard({ serial }: { serial: string }) {
  const { t } = useTranslation();
  const [port, setPort] = useState('5555');
  const [state, setState] = useState<State>({ kind: 'idle' });
  // Whether the details/log panel is expanded.
  const [detailsOpen, setDetailsOpen] = useState(false);
  // Tracks which IP row is mid-"connect from this app" so the
  // button shows a spinner instead of being clickable twice.
  const [connectingIp, setConnectingIp] = useState<string | null>(null);
  const [reconnecting, setReconnecting] = useState(false);
  // Tooltip/state for the "copied" flash on the connect command.
  const [copied, setCopied] = useState<string | null>(null);
  const copyTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  // Last-resort manual IP input -- surfaced when the auto probe
  // finds nothing (e.g. IPv6-only network, missing iproute2, ...).
  const [manualIp, setManualIp] = useState('');

  const prevSerial = useRef(serial);
  useEffect(() => {
    if (prevSerial.current !== serial) {
      prevSerial.current = serial;
      setState({ kind: 'idle' });
      setManualIp('');
    }
  }, [serial]);

  useEffect(() => () => {
    if (copyTimer.current) clearTimeout(copyTimer.current);
  }, []);

  const canRun = /^\d+$/.test(port) && parseInt(port, 10) > 0 && serial.length > 0;

  async function run() {
    if (!canRun) return;
    setState({ kind: 'running' });

    // Step 1: probe IPs FIRST. Reasoning: `adb tcpip` is a
    // device-side command that succeeds regardless of whether
    // the device is on WiFi, but it's pointless without a
    // discoverable IP. Running the probes first means we fail
    // fast (no ~2s wait for a useless tcpip) and the diagnostic
    // table the user sees is purely about the IP question.
    const { candidates, probes } = await collectCandidateIps(serial);
    if (candidates.length === 0) {
      setState({
        kind: 'error',
        // No tcpipOut yet -- we never ran tcpip because there'd
        // be nothing to connect to. Keep the message terse so
        // the diagnostic table (rendered below) does the talking.
        message: t('tools.wirelessTcpip.noIpFoundNoTcpip'),
        probes,
      });
      return;
    }

    // Step 2: tcpip with port fallback. User-supplied port
    // first, then FALLBACK_PORTS. We keep the IPs from step 1
    // visible in the UI even when tcpip fails so the user can
    // still try connecting -- on a phone that's already in tcpip
    // mode from a previous session, the new tcpip call will fail
    // ("address in use") but the old listener is still serving.
    const userPort = parseInt(port, 10);
    const tried = new Set<number>();
    const order: number[] = [];
    for (const p of [userPort, ...FALLBACK_PORTS]) {
      if (!tried.has(p)) {
        tried.add(p);
        order.push(p);
      }
    }

    let lastError: string | null = null;
    let chosenPort: number | null = null;
    let tcpipOut = '';
    for (const p of order) {
      try {
        tcpipOut = await tryTcpip(serial, p);
        chosenPort = p;
        break;
      } catch (e) {
        lastError = String(e);
      }
    }
    if (chosenPort === null) {
      // tcpip failed on every fallback port -- but we DID find
      // IPs, so surface the partial result: a "tcpip failed but
      // IP was found" state with the candidates still usable.
      // The error panel renders both the failure message AND
      // the IP + connect buttons (because the previous tcpip
      // session might still be listening on one of the fallback
      // ports).
      setState({
        kind: 'error',
        message: t('tools.wirelessTcpip.tcpipFailedButIpKnown', {
          error: lastError ?? t('tools.wirelessTcpip.tcpipFailed'),
        }),
        probes,
        candidates,
      });
      return;
    }

    const primary = candidates[0];
    if (!primary) {
      setState({ kind: 'error', message: t('tools.wirelessTcpip.noIpFoundNoTcpip'), probes });
      return;
    }

    setState({
      kind: 'ok',
      ip: primary.ip,
      port: chosenPort,
      tcpipOut,
      candidates,
      probes,
    });
    toast.success(
      t('tools.wirelessTcpip.success', { ip: primary.ip, port: chosenPort }),
    );
  }

  function adoptManualIp() {
    if (!BARE_IPV4_RE.test(manualIp.trim())) return;
    // Adopt the manual IP by injecting it into the candidates
    // list. We use the `ok` state so the rest of the UI (copy +
    // connect buttons) works unchanged.
    const ip = manualIp.trim();
    const tcpipOut = state.kind === 'ok' ? state.tcpipOut : '';
    const portNum = state.kind === 'ok' ? state.port : parseInt(port, 10);
    const probes = state.kind === 'ok' ? state.probes : [];
    const existing = state.kind === 'ok' ? state.candidates : [];
    setState({
      kind: 'ok',
      ip,
      port: portNum,
      tcpipOut,
      candidates: [{ ip, source: 'manual' }, ...existing.filter((c) => c.ip !== ip)],
      probes,
    });
    setManualIp('');
    toast.success(t('tools.wirelessTcpip.manualAdopted', { ip }));
  }

  async function connectFromApp(ip: string, p: number) {
    setConnectingIp(ip);
    try {
      const out = await adbConnect(ip, p);
      toast.success(t('tools.wirelessTcpip.connectedFromApp', { out }));
    } catch (e) {
      toast.error(t('tools.wirelessTcpip.connectFailed', { error: String(e) }));
    } finally {
      setConnectingIp(null);
    }
  }

  async function reconnectSerial() {
    setReconnecting(true);
    try {
      const out = await adbReconnect(serial);
      toast.success(t('tools.wirelessTcpip.reconnected', { out }));
    } catch (e) {
      toast.error(t('tools.wirelessTcpip.connectFailed', { error: String(e) }));
    } finally {
      setReconnecting(false);
    }
  }

  function copyConnectCommand(ip: string, p: number) {
    const cmd = `adb connect ${ip}:${p}`;
    if (typeof navigator !== 'undefined' && navigator.clipboard) {
      void navigator.clipboard.writeText(cmd).then(
        () => {
          setCopied(ip);
          if (copyTimer.current) clearTimeout(copyTimer.current);
          copyTimer.current = setTimeout(() => setCopied(null), 1500);
        },
        () => {/* swallow */},
      );
    }
  }

  const detailsProbes = state.kind === 'ok' || state.kind === 'error' ? state.probes : [];
  const showProbes = detailsProbes.length > 0;

  return (
    <Card className="overflow-hidden">
      <CardContent className="space-y-3 p-3">
        <div className="flex items-center gap-2.5">
          <div className="grid h-8 w-8 shrink-0 place-items-center rounded-md bg-brand/15 text-brand">
            <Wifi className="h-4 w-4" />
          </div>
          <div className="min-w-0 flex-1">
            <div className="flex items-center gap-1.5">
              <h3 className="truncate text-sm font-medium leading-tight text-text-0">
                {t('tools.wirelessTcpip.title')}
              </h3>
              {state.kind === 'ok' && (
                <span
                  className="inline-flex h-4 w-4 shrink-0 items-center justify-center rounded-full bg-success/15 text-success"
                  aria-label={t('tools.miuiUsbInstall.resultSuccess')}
                  title={t('tools.miuiUsbInstall.resultSuccess')}
                >
                  <Check className="h-3 w-3" />
                </span>
              )}
              {state.kind === 'error' && (
                state.candidates && state.candidates.length > 0 ? (
                  <span
                    className="inline-flex h-4 w-4 shrink-0 items-center justify-center rounded-full bg-warning/15 text-warning"
                    aria-label={t('tools.wirelessTcpip.partialWarning')}
                    title={t('tools.wirelessTcpip.partialWarning')}
                  >
                    <AlertTriangle className="h-3 w-3" />
                  </span>
                ) : (
                  <span
                    className="inline-flex h-4 w-4 shrink-0 items-center justify-center rounded-full bg-danger/15 text-danger"
                    aria-label={t('tools.miuiUsbInstall.resultFailed')}
                    title={t('tools.miuiUsbInstall.resultFailed')}
                  >
                    <AlertTriangle className="h-3 w-3" />
                  </span>
                )
              )}
            </div>
            <p className="truncate text-[11px] leading-tight text-text-2">
              {t('tools.wirelessTcpip.subtitle')}
            </p>
          </div>
        </div>

        <div className="flex items-end gap-2">
          <div className="min-w-0 flex-1 space-y-1">
            <label className="text-[11px] text-text-2" htmlFor="tcpip-port">
              {t('tools.wirelessTcpip.port')}
            </label>
            <Input
              id="tcpip-port"
              value={port}
              onChange={(e) => setPort(e.target.value.replace(/[^\d]/g, ''))}
              placeholder="5555"
              inputMode="numeric"
              maxLength={5}
              disabled={state.kind === 'running'}
              className="h-8 text-xs"
            />
          </div>
          <Button
            onClick={() => void run()}
            disabled={!canRun || state.kind === 'running'}
            size="sm"
            className="h-8 gap-1 px-3"
          >
            {state.kind === 'running' ? (
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
            ) : state.kind === 'ok' ? (
              <RefreshCw className="h-3.5 w-3.5" />
            ) : (
              <Wifi className="h-3.5 w-3.5" />
            )}
            {state.kind === 'running'
              ? t('tools.wirelessTcpip.busy')
              : state.kind === 'ok'
              ? t('tools.wirelessTcpip.restart')
              : t('tools.wirelessTcpip.run')}
          </Button>
        </div>

        {state.kind === 'ok' && (
          <div className="space-y-2 rounded-md border border-success/30 bg-success/5 p-2">
            <div className="flex items-center justify-between text-[11px] text-text-2">
              <span>{t('tools.wirelessTcpip.candidateIps')}</span>
              <Badge variant="success" className="h-4 px-1 py-0 text-[10px] leading-none">
                {t('tools.wirelessTcpip.listening', { port: state.port })}
              </Badge>
            </div>
            <ul className="space-y-1.5">
              {state.candidates.map((c) => {
                const isCopied = copied === c.ip;
                const isConnecting = connectingIp === c.ip;
                return (
                  <li
                    key={`${c.source}-${c.ip}-${c.iface ?? ''}`}
                    className="flex flex-wrap items-center gap-2 rounded border border-border bg-bg-1 px-2 py-1.5"
                  >
                    <div className="min-w-0 flex-1">
                      <div className="flex items-center gap-1.5">
                        <span className="font-mono text-[12px] text-text-0">{c.ip}</span>
                        <Badge variant="secondary" className="h-4 px-1 py-0 text-[10px] leading-none">
                          {t(`tools.wirelessTcpip.source.${c.source}`)}
                          {c.iface ? ` · ${c.iface}` : ''}
                        </Badge>
                      </div>
                      <code className="block truncate font-mono text-[10px] text-text-2">
                        adb connect {c.ip}:{state.port}
                      </code>
                    </div>
                    <Button
                      variant="outline"
                      size="sm"
                      className="h-6 gap-1 px-2"
                      onClick={() => copyConnectCommand(c.ip, state.port)}
                    >
                      {isCopied ? (
                        <Check className="h-3 w-3 text-success" />
                      ) : (
                        <Clipboard className="h-3 w-3" />
                      )}
                      {isCopied ? t('tools.wirelessTcpip.copied') : t('tools.wirelessTcpip.copy')}
                    </Button>
                    <Button
                      variant="default"
                      size="sm"
                      className="h-6 gap-1 px-2"
                      onClick={() => void connectFromApp(c.ip, state.port)}
                      disabled={isConnecting}
                    >
                      {isConnecting ? (
                        <Loader2 className="h-3 w-3 animate-spin" />
                      ) : (
                        <PlugZap className="h-3 w-3" />
                      )}
                      {t('tools.wirelessTcpip.connectFromApp')}
                    </Button>
                  </li>
                );
              })}
            </ul>
            <p className="text-[11px] text-text-2">
              {t('tools.wirelessTcpip.hint')}
            </p>
            <div className="flex items-center justify-end border-t border-border pt-2">
              <Button
                variant="ghost"
                size="sm"
                className="h-6 gap-1 px-2 text-text-2"
                onClick={() => void reconnectSerial()}
                disabled={reconnecting}
              >
                {reconnecting ? (
                  <Loader2 className="h-3 w-3 animate-spin" />
                ) : (
                  <RefreshCw className="h-3 w-3" />
                )}
                {t('tools.wirelessTcpip.reconnect')}
              </Button>
            </div>
          </div>
        )}

        {state.kind === 'error' && (
          <div className="space-y-2 rounded-md border border-danger/30 bg-danger/5 p-2">
            <div className="text-[11px] text-danger">{state.message}</div>
            {/* One-line per-probe diagnostic. Each row shows
                whether the command returned an IP and which one,
                or `empty` / `threw`. We render this even when
                the error was a port-fallback failure (probes
                list is empty in that case, so the loop just
                produces nothing). This is the single most useful
                surface for diagnosing "no IP found" -- the user
                sees which probe failed and why without expanding
                anything. */}
            {state.probes.length > 0 && (
              <ul className="space-y-0.5 rounded border border-danger/20 bg-bg-1 p-2 font-mono text-[10px]">
                {state.probes.map((p, i) => (
                  <li key={`diag-${i}`} className="flex items-start gap-2">
                    <span
                      className={
                        p.status === 'hit'
                          ? 'text-success'
                          : p.status === 'threw'
                          ? 'text-danger'
                          : 'text-text-2'
                      }
                    >
                      {p.status === 'hit' ? '\u2713' : p.status === 'threw' ? '\u2717' : '\u00b7'}
                    </span>
                    <span className="min-w-0 flex-1 text-text-1">
                      {p.source}
                      {p.iface ? ` (${p.iface})` : ''}
                    </span>
                    <span className="shrink-0 text-text-2">
                      {p.status === 'hit' ? p.ip : p.status}
                    </span>
                  </li>
                ))}
              </ul>
            )}
            {/* When tcpip itself failed but the IP probe did
                surface candidates (e.g. the device was already
                listening from a previous run), render the same
                per-candidate list as the success branch so the
                user can still try to connect -- the previous
                tcpip session might still be serving. We use the
                user-picked port (best effort); an explicit port
                // would be nicer but we don't track which one
                // might still be listening. */}
            {state.candidates && state.candidates.length > 0 && (
              <ul className="space-y-1.5 rounded border border-warning/30 bg-bg-1 p-2">
                {state.candidates.map((c) => (
                  <li
                    key={`err-${c.source}-${c.ip}-${c.iface ?? ''}`}
                    className="flex flex-wrap items-center gap-2 rounded border border-border bg-bg-1 px-2 py-1.5"
                  >
                    <div className="min-w-0 flex-1">
                      <div className="flex items-center gap-1.5">
                        <span className="font-mono text-[12px] text-text-0">{c.ip}</span>
                        <Badge variant="secondary" className="h-4 px-1 py-0 text-[10px] leading-none">
                          {t(`tools.wirelessTcpip.source.${c.source}`)}
                          {c.iface ? ` · ${c.iface}` : ''}
                        </Badge>
                      </div>
                      <code className="block truncate font-mono text-[10px] text-text-2">
                        adb connect {c.ip}:{parseInt(port, 10)}
                      </code>
                    </div>
                    <Button
                      variant="outline"
                      size="sm"
                      className="h-6 gap-1 px-2"
                      onClick={() => copyConnectCommand(c.ip, parseInt(port, 10))}
                    >
                      <Clipboard className="h-3 w-3" />
                      {t('tools.wirelessTcpip.copy')}
                    </Button>
                    <Button
                      variant="default"
                      size="sm"
                      className="h-6 gap-1 px-2"
                      onClick={() => void connectFromApp(c.ip, parseInt(port, 10))}
                    >
                      <PlugZap className="h-3 w-3" />
                      {t('tools.wirelessTcpip.connectFromApp')}
                    </Button>
                  </li>
                ))}
              </ul>
            )}
            {/* Last-resort manual IP entry. Only shown when the
                error is the "no IP found" branch (probes
                collection ran but came up empty); port-fallback
                failures skip this. */}
            {state.probes.length > 0 && (
              <div className="flex items-end gap-2 border-t border-danger/20 pt-2">
                <div className="min-w-0 flex-1 space-y-1">
                  <label
                    className="inline-flex items-center gap-1 text-[11px] text-text-2"
                    htmlFor="manual-ip"
                  >
                    <Keyboard className="h-3 w-3" />
                    {t('tools.wirelessTcpip.manualIp')}
                  </label>
                  <Input
                    id="manual-ip"
                    value={manualIp}
                    onChange={(e) => setManualIp(e.target.value)}
                    placeholder="192.168.1.10"
                    inputMode="numeric"
                    disabled={state.kind !== 'error'}
                    className="h-7 font-mono text-xs"
                  />
                </div>
                <Button
                  variant="outline"
                  size="sm"
                  className="h-7 gap-1 px-2"
                  onClick={() => adoptManualIp()}
                  disabled={!BARE_IPV4_RE.test(manualIp.trim())}
                >
                  {t('tools.wirelessTcpip.manualUse')}
                </Button>
              </div>
            )}
          </div>
        )}

        {showProbes && (
          <div className="space-y-1">
            <button
              type="button"
              onClick={() => setDetailsOpen((v) => !v)}
              aria-expanded={detailsOpen}
              className="inline-flex items-center gap-1 text-[11px] text-text-2 hover:text-text-1"
            >
              {detailsOpen ? (
                <ChevronUp className="h-3 w-3" />
              ) : (
                <ChevronDown className="h-3 w-3" />
              )}
              {detailsOpen
                ? t('tools.wirelessTcpip.hideDetails')
                : t('tools.wirelessTcpip.showDetails')}
            </button>
            {detailsOpen && (
              <div className="max-h-72 space-y-1 overflow-y-auto rounded-md border border-border bg-bg-2 p-2">
                {detailsProbes.map((p, i) => (
                  <div key={`${p.source}-${i}`} className="space-y-0.5">
                    <div className="font-mono text-[10px] text-text-2">{p.source}</div>
                    <pre className="overflow-x-auto whitespace-pre-wrap break-all rounded border border-border bg-bg-1 p-1.5 font-mono text-[10px] leading-relaxed text-text-1">
                      {p.raw}
                    </pre>
                  </div>
                ))}
              </div>
            )}
          </div>
        )}
      </CardContent>
    </Card>
  );
}
