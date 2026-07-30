import { useTranslation } from 'react-i18next';
import '@/i18n';
import {
  Activity,
  Battery,
  Power,
  BatteryCharging,
  BatteryFull,
  Cpu,
  HardDrive,
  Info,
  Loader2,
  MemoryStick,
  MonitorSmartphone,
  RefreshCw,
  Router,
  Settings2,
  Wifi,
  WifiOff,
} from 'lucide-react';
import { Card, CardContent } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { cn } from '@/lib/utils';
import type { DeviceSystemInfo } from '@/ipc/adb';

type Props = {
  serial: string | null;
  info: DeviceSystemInfo | null;
  loading: boolean;
  error: string | null;
  lastUpdated: number | null;
  onRefresh: () => void;
};

type Row = {
  label: string;
  value: string | null | undefined;
};

type Section = {
  key: string;
  icon: React.ReactNode;
  title: string;
  rows: Row[];
};

const NA = '—';

// Friendly translation for the most common ARM64 CPU features. Any
// unknown token is left as-is so we never silently drop data.
const CPU_FEATURE_ZH: Record<string, string> = {
  fp: '浮点',
  asimd: 'NEON SIMD',
  aes: 'AES',
  pmull: 'AES-GCM',
  sha1: 'SHA-1',
  sha2: 'SHA-256',
  sha3: 'SHA-3',
  sha512: 'SHA-512',
  crc32: 'CRC32',
  atomics: 'LSE 原子操作',
  fphp: 'FP16',
  asimdhp: 'NEON FP16',
  asimddp: 'NEON 点积 (AI)',
  asimdrdm: 'SIMD 舍入乘法',
  lrcpc: 'RCpc 弱一致加载',
  dcpop: '数据缓存持久化',
  evtstrm: '事件流',
  cpuid: 'CPUID',
  sve: '可伸缩向量 (SVE)',
  sve2: 'SVE2',
  i8mm: 'INT8 矩阵乘 (AI)',
  bf16: 'BF16',
  fcma: '复数 MAC',
  jscvt: 'JS 转换',
  frint: '舍入模式',
  paca: 'PAuth (指令)',
  pacg: 'PAuth (通用)',
  dit: '数据独立时序',
  rng: '硬件随机数',
  sb: '影子分支',
  ssbs: '影子分支安全',
  sm3: '国密 SM3',
  sm4: '国密 SM4',
};

function decodeCpuFeatures(raw: string | null | undefined): string | null {
  if (!raw) return null;
  const tokens = raw.trim().split(/\s+/).filter(Boolean);
  if (tokens.length === 0) return null;
  return tokens.map((tok) => CPU_FEATURE_ZH[tok] ?? tok).join(' · ');
}

function v(value: string | null | undefined): string {
  if (value === null || value === undefined) return NA;
  const t = String(value).trim();
  if (!t) return NA;
  // Backend placeholders that Android emits when a value is
  // privacy-gated (SSID on Android 10+, etc). Don't paint them.
  if (
    t === 'null' ||
    t === '<none>' ||
    t === '<unknown ssid>' ||
    t === 'unknown'
  ) {
    return NA;
  }
  return t;
}

function parseBatteryLevel(info: DeviceSystemInfo | null): number | null {
  if (!info || !info.batteryLevel) return null;
  const n = Number.parseInt(info.batteryLevel, 10);
  return Number.isFinite(n) ? n : null;
}

function batteryTone(level: number | null): 'danger' | 'warning' | 'success' {
  if (level === null) return 'warning';
  if (level <= 20) return 'danger';
  if (level <= 40) return 'warning';
  return 'success';
}

function BatteryIcon({ level, charging }: { level: number | null; charging: boolean }) {
  if (charging) return <BatteryCharging className="h-5 w-5 text-success" />;
  if (level === null) return <Battery className="h-5 w-5 text-text-2" />;
  if (level >= 90) return <BatteryFull className="h-5 w-5 text-success" />;
  if (level <= 20) return <Battery className="h-5 w-5 text-danger" />;
  return <Battery className="h-5 w-5 text-warning" />;
}

function NetworkIcon({ type, online }: { type: string | null; online: boolean }) {
  if (!online) return <WifiOff className="h-5 w-5 text-text-2" />;
  if (type === 'Wi-Fi') return <Wifi className="h-5 w-5 text-brand" />;
  return <Router className="h-5 w-5 text-brand" />;
}

function isCharging(info: DeviceSystemInfo | null): boolean {
  if (!info) return false;
  const status = (info.batteryStatus ?? '').toLowerCase();
  const plugged = (info.batteryPlugged ?? '').toLowerCase();
  return (
    status.includes('charging') ||
    status.includes('full') ||
    plugged.includes('ac') ||
    plugged.includes('usb') ||
    plugged.includes('wireless')
  );
}

function buildSections(info: DeviceSystemInfo, t: (k: string) => string): Section[] {
  return [
    {
      key: 'hardware',
      icon: <MonitorSmartphone className="h-5 w-5 text-brand" />,
      title: t('adb.sysInfo.hardware'),
      rows: [
        { label: t('adb.sysInfo.manufacturer'), value: info.manufacturer },
        { label: t('adb.sysInfo.brand'), value: info.brand },
        { label: t('adb.sysInfo.model'), value: info.model },
        { label: t('adb.sysInfo.deviceName'), value: info.device },
        { label: t('adb.sysInfo.hardwareName'), value: info.hardware },
        { label: t('adb.sysInfo.platform'), value: info.platform },
        { label: t('adb.sysInfo.serialNo'), value: info.serial },
        { label: t('adb.sysInfo.bootloader'), value: info.bootloader },
        { label: t('adb.sysInfo.fingerprint'), value: info.fingerprint },
      ],
    },
    {
      key: 'screen',
      icon: <MonitorSmartphone className="h-5 w-5 text-info" />,
      title: t('adb.sysInfo.screen'),
      rows: [
        { label: t('adb.sysInfo.resolution'), value: info.screenSize },
        {
          label: t('adb.sysInfo.density'),
          value: info.screenDensity ? `${info.screenDensity} dpi` : null,
        },
        { label: t('adb.sysInfo.refreshRate'), value: info.screenRefreshRate },
        { label: t('adb.sysInfo.physicalSize'), value: info.physicalSize },
        { label: t('adb.sysInfo.rotation'), value: info.rotation },
        { label: t('adb.sysInfo.screenState'), value: info.screenState },
      ],
    },
    {
      key: 'system',
      icon: <Settings2 className="h-5 w-5 text-text-1" />,
      title: t('adb.sysInfo.system'),
      rows: [
        { label: t('adb.sysInfo.androidVersion'), value: info.androidRelease },
        { label: t('adb.sysInfo.sdk'), value: info.androidSdk },
        { label: t('adb.sysInfo.securityPatch'), value: info.securityPatch },
        { label: t('adb.sysInfo.buildId'), value: info.buildId },
        { label: t('adb.sysInfo.buildType'), value: info.buildType },
        { label: t('adb.sysInfo.kernel'), value: info.kernelVersion },
        { label: t('adb.sysInfo.jvm'), value: info.javaVm },
        { label: t('adb.sysInfo.abi'), value: info.abi },
        { label: t('adb.sysInfo.abiList'), value: info.abiList },
      ],
    },
    {
      // Single card carrying CPU + GPU under the title "芯片 / Chip".
      // Rows are visually subdivided by an italic subtitle instead of
      // a second card so the user reads them as one SoC's profile.
      key: 'chip',
      icon: <Cpu className="h-5 w-5 text-warning" />,
      title: t('adb.sysInfo.cpu'),
      rows: [
        { label: t('adb.sysInfo.cpuCores'), value: info.cpuCores },
        { label: t('adb.sysInfo.cpuHardware'), value: info.cpuHardware },
        { label: t('adb.sysInfo.cpuMaxFreq'), value: info.cpuMaxFreq },
        {
          label: t('adb.sysInfo.cpuFeatures'),
          value: decodeCpuFeatures(info.cpuFeatures),
        },
        // ---- GPU block ----
        { label: t('adb.sysInfo.gpuVendor'), value: info.gpuVendor },
        { label: t('adb.sysInfo.gpuRenderer'), value: info.gpuRenderer },
        {
          label: t('adb.sysInfo.gpuOpenglesVersion'),
          value: info.gpuOpenglesVersion,
        },
        {
          label: t('adb.sysInfo.gpuVulkanVersion'),
          value: info.gpuVulkanVersion,
        },
        { label: t('adb.sysInfo.gpuDriver'), value: info.gpuDriver },
      ],
    },
    {
      key: 'memory',
      icon: <MemoryStick className="h-5 w-5 text-brand" />,
      title: t('adb.sysInfo.memory'),
      rows: [
        { label: t('adb.sysInfo.ramTotal'), value: info.ramTotal },
        { label: t('adb.sysInfo.ramAvailable'), value: info.ramAvailable },
      ],
    },
    {
      key: 'storage',
      icon: <HardDrive className="h-5 w-5 text-text-1" />,
      title: t('adb.sysInfo.storage'),
      rows: [
        { label: t('adb.sysInfo.storageTotal'), value: info.storageTotal },
        {
          label: t('adb.sysInfo.storageAvailable'),
          value: info.storageAvailable,
        },
      ],
    },
    {
      key: 'network',
      icon: (
        <NetworkIcon
          type={info.networkType ?? null}
          online={info.networkType !== 'Offline'}
        />
      ),
      title: t('adb.sysInfo.network'),
      rows: [
        { label: t('adb.sysInfo.networkType'), value: info.networkType },
        { label: t('adb.sysInfo.wifiSsid'), value: info.wifiSsid },
        { label: t('adb.sysInfo.wifiIp'), value: info.wifiIp },
        { label: t('adb.sysInfo.ipv4'), value: info.ipv4 },
        {
          label: t('adb.sysInfo.wifiSignal'),
          value: info.wifiSignal ? `${info.wifiSignal} dBm` : null,
        },
        {
          label: t('adb.sysInfo.wifiLinkSpeed'),
          value: info.wifiLinkSpeed ? `${info.wifiLinkSpeed} Mbps` : null,
        },
        { label: t('adb.sysInfo.wifiFrequency'), value: info.wifiFrequency },
        { label: t('adb.sysInfo.operator'), value: info.operator },
        { label: t('adb.sysInfo.airplaneMode'), value: info.airplaneMode },
      ],
    },
    {
      key: 'battery',
      icon: (
        <BatteryIcon
          level={parseBatteryLevel(info)}
          charging={isCharging(info)}
        />
      ),
      title: t('adb.sysInfo.battery'),
      rows: [
        {
          label: t('adb.sysInfo.batteryLevel'),
          value: info.batteryLevel ? `${info.batteryLevel}%` : null,
        },
        { label: t('adb.sysInfo.batteryStatus'), value: info.batteryStatus },
        { label: t('adb.sysInfo.batteryHealth'), value: info.batteryHealth },
        { label: t('adb.sysInfo.batteryTemp'), value: info.batteryTemp },
        { label: t('adb.sysInfo.batteryVoltage'), value: info.batteryVoltage },
        { label: t('adb.sysInfo.batteryTech'), value: info.batteryTechnology },
        { label: t('adb.sysInfo.batteryPlugged'), value: info.batteryPlugged },
      ],
    },
    {
      key: 'runtime',
      icon: <Power className="h-5 w-5 text-text-1" />,
      title: t('adb.sysInfo.runtime'),
      rows: [
        { label: t('adb.sysInfo.uptime'), value: info.uptime },
        { label: t('adb.sysInfo.bootTime'), value: info.bootTime },
        { label: t('adb.sysInfo.timezone'), value: info.timezone },
        { label: t('adb.sysInfo.locale'), value: info.locale },
        { label: t('adb.sysInfo.selinux'), value: info.selinux },
        { label: t('adb.sysInfo.foregroundApp'), value: info.foregroundApp },
      ],
    },
  ];
}

export function AdbSystemInfoTab({
  serial,
  info,
  loading,
  error,
  onRefresh,
}: Props) {
  const { t } = useTranslation();

  if (!serial) {
    return (
      <Card>
        <CardContent className="py-6 text-sm text-text-2">
          {t('adb.noDeviceSelected')}
        </CardContent>
      </Card>
    );
  }

  const batLevel = parseBatteryLevel(info);
  const chargingNow = isCharging(info);

  return (
    <div className="space-y-4">
      <Card>
        <CardContent className="flex flex-wrap items-center gap-3">
          <div className="flex items-center gap-2 text-sm font-semibold text-text-0">
            <Activity className="h-4 w-4 text-brand" />
            {t('adb.sysInfo.title')}
          </div>
          <div className="ml-2 flex flex-wrap items-center gap-2 text-xs text-text-2">
            {info?.androidRelease && (
              <Badge variant="secondary">
                {t('adb.sysInfo.androidBadge', {
                  release: info.androidRelease,
                  sdk: info.androidSdk ?? '?',
                })}
              </Badge>
            )}
            {batLevel !== null && (
              <Badge variant={batteryTone(batLevel)}>
                {chargingNow ? '⚡ ' : ''}
                {batLevel}%
              </Badge>
            )}
            {info?.uptime && (
              <Badge variant="outline">
                {t('adb.sysInfo.uptimeLabel', { value: info.uptime })}
              </Badge>
            )}
          </div>
          <div className="ml-auto flex items-center gap-2">
            <Button
              size="sm"
              variant="outline"
              onClick={onRefresh}
              disabled={loading}
              title={t('adb.refresh')}
            >
              {loading ? (
                <Loader2 className="h-3 w-3 animate-spin" />
              ) : (
                <RefreshCw className="h-3 w-3" />
              )}
              {t('adb.refresh')}
            </Button>
          </div>
        </CardContent>
      </Card>

      {info ? (
        // ---- data path ----
        <div className="grid grid-cols-1 gap-4 md:grid-cols-2 xl:grid-cols-3">
          {buildSections(info, t).map((section) => (
            <Card key={section.key}>
              <CardContent className="space-y-3">
                <div className="flex items-center gap-2">
                  {section.icon}
                  <h3 className="text-sm font-semibold text-text-0">
                    {section.title}
                  </h3>
                </div>
                <dl className="divide-y divide-border rounded-md border border-border bg-bg-2/40">
                  {section.rows.map((row) => (
                    <div
                      key={row.label}
                      className="flex items-center justify-between gap-2 px-3 py-1.5 text-xs"
                    >
                      <dt className="shrink-0 text-text-2">{row.label}</dt>
                      <dd
                        className={cn(
                          'break-all text-right font-mono text-text-0',
                          v(row.value) === NA && 'text-text-2',
                        )}
                      >
                        {v(row.value)}
                      </dd>
                    </div>
                  ))}
                </dl>
              </CardContent>
            </Card>
          ))}
        </div>
      ) : error ? (
        // ---- error path ----
        // Error wins over loading so a failed fetch shows a single,
        // clear error card rather than stacking with a spinner.
        <Card className="border-danger">
          <CardContent className="flex items-start gap-3 text-sm">
            <Info className="mt-0.5 h-5 w-5 shrink-0 text-danger" />
            <div className="min-w-0 flex-1 text-danger">{error}</div>
          </CardContent>
        </Card>
      ) : loading ? (
        // ---- loading path ----
        <Card>
          <CardContent className="flex items-center gap-2 py-6 text-sm text-text-2">
            <Loader2 className="h-4 w-4 animate-spin" />
            {t('adb.sysInfo.loading')}
          </CardContent>
        </Card>
      ) : (
        // ---- empty path ----
        <Card>
          <CardContent className="py-6 text-sm text-text-2">
            {t('adb.sysInfo.empty')}
          </CardContent>
        </Card>
      )}
    </div>
  );
}
