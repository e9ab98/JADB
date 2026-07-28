import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import '@/i18n';
import { Info } from 'lucide-react';
import { Card, CardContent } from '@/components/ui/card';
import { cn } from '@/lib/utils';
import type { ApkInfo, VolumeEntry, VolumeStats } from '@/ipc/analyze';

type Slice = {
  label: string;
  size: number;
  color: string;
};

const SLICE_COLORS: Record<keyof VolumeStats, string> = {
  dex: '#4caf50',
  lib: '#2196f3',
  res: '#ff9800',
  assets: '#9c27b0',
  manifest: '#607d8b',
  arsc: '#f44336',
  other: '#9e9e9e',
  lib_breakdown: '#9e9e9e', // never used
  redundant_files: '#9e9e9e', // never used
  waste_size: '#9e9e9e', // never used
};

function formatSize(bytes: number): string {
  if (bytes === 0) return '0 B';
  const k = 1024;
  const units = ['B', 'KB', 'MB', 'GB'];
  const i = Math.min(Math.floor(Math.log(bytes) / Math.log(k)), units.length - 1);
  const v = bytes / Math.pow(k, i);
  return `${v.toFixed(2)} ${units[i]}`;
}

function buildSlices(stats: VolumeStats): Slice[] {
  const keys: Array<{ key: keyof VolumeStats; label: string }> = [
    { key: 'dex', label: 'Dex' },
    { key: 'lib', label: 'Libs' },
    { key: 'res', label: 'Res' },
    { key: 'assets', label: 'Assets' },
    { key: 'arsc', label: 'Arsc' },
    { key: 'manifest', label: 'Manifest' },
    { key: 'other', label: 'Other' },
  ];
  return keys
    .map(({ key, label }) => ({
      label,
      size: stats[key] as number,
      color: SLICE_COLORS[key] ?? '#9e9e9e',
    }))
    .filter((d) => d.size > 0)
    .sort((a, b) => b.size - a.size);
}

function PieChart({ slices, total }: { slices: Slice[]; total: number }) {
  const [hover, setHover] = useState<Slice | null>(null);
  const safeTotal = total || 1;
  let cumulative = 0;
  const radius = 90;
  const cx = 90;
  const cy = 90;
  const paths = slices.map((d, i) => {
    const start = (cumulative / safeTotal) * 2 * Math.PI;
    cumulative += d.size;
    const end = (cumulative / safeTotal) * 2 * Math.PI;
    const x1 = cx + radius * Math.sin(start);
    const y1 = cy - radius * Math.cos(start);
    const x2 = cx + radius * Math.sin(end);
    const y2 = cy - radius * Math.cos(end);
    const largeArc = d.size / safeTotal > 0.5 ? 1 : 0;
    const path = `M ${cx} ${cy} L ${x1} ${y1} A ${radius} ${radius} 0 ${largeArc} 1 ${x2} ${y2} Z`;
    return (
      <path
        key={`${d.label}-${i}`}
        d={path}
        fill={d.color}
        stroke="var(--bg-1, #fff)"
        strokeWidth={2}
        style={{
          transformOrigin: `${cx}px ${cy}px`,
          cursor: 'pointer',
          transition: 'transform 0.2s',
        }}
        onMouseEnter={() => setHover(d)}
        onMouseLeave={() => setHover(null)}
      />
    );
  });
  const center = hover
    ? { label: hover.label, value: `${((hover.size / safeTotal) * 100).toFixed(1)}%`, color: hover.color }
    : { label: 'STATS', value: `${slices.length} Types`, color: 'var(--brand, #2563eb)' };

  return (
    <div className="relative h-[180px] w-[180px] shrink-0">
      <svg viewBox="0 0 180 180" className="h-full w-full overflow-visible">
        {paths}
      </svg>
      <div className="pointer-events-none absolute inset-0 flex flex-col items-center justify-center">
        <span
          className="text-[10px] font-bold uppercase tracking-widest"
          style={{ color: center.color, opacity: 0.7 }}
        >
          {center.label}
        </span>
        <span className="text-base font-extrabold" style={{ color: center.color }}>
          {center.value}
        </span>
      </div>
    </div>
  );
}

function LargestFilesTable({ entries }: { entries: VolumeEntry[] }) {
  return (
    <div className="overflow-hidden rounded-xl border border-border">
      <table className="w-full text-xs">
        <tbody>
          {entries.map((f) => (
            <tr key={f.name} className="border-t border-border/60 first:border-t-0">
              <td className="max-w-[400px] truncate px-3 py-2 font-mono text-text-1" title={f.name}>
                {f.name}
              </td>
              <td className="w-24 px-3 py-2 text-right text-text-0">{formatSize(f.size)}</td>
              <td className="w-32 px-3 py-2">
                <div className="h-1.5 overflow-hidden rounded-full bg-bg-2">
                  <div
                    className="h-full rounded-full bg-brand"
                    style={{ width: `${Math.min(100, f.ratio).toFixed(1)}%` }}
                  />
                </div>
              </td>
              <td className="w-14 px-3 py-2 text-right text-[10px] font-bold text-text-2">
                {f.ratio.toFixed(1)}%
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

type Props = { info: ApkInfo };

export function VolumePanel({ info }: Props) {
  const { t } = useTranslation();
  const stats = info.volume_stats;
  const total = info.volume_total_size ?? 0;

  if (!stats) {
    return (
      <div className="rounded-xl border border-dashed border-border p-8 text-center text-sm text-text-2">
        {t('analyze.noVolumeData')}
      </div>
    );
  }

  const slices = buildSlices(stats);
  const libAbis = Object.entries(stats.lib_breakdown || {});
  const redundant = (stats.redundant_files || []).slice(0, 5);
  const largest = (info.largest_files || []).slice(0, 10);
  const insights = info.insights || [];

  return (
    <div className="space-y-4">
      <Card>
        <CardContent className="flex flex-wrap items-center justify-between gap-3 p-5">
          <h3 className="text-base font-semibold text-text-0">
            {t('analyze.volumeTotalCompressed')}
          </h3>
          <div className="text-right">
            <div className="text-xl font-extrabold text-brand">{formatSize(total)}</div>
            {stats.waste_size > 0 && (
              <div className="mt-0.5 text-[11px] text-danger">
                {t('analyze.volumeWaste', { size: formatSize(stats.waste_size) })}
              </div>
            )}
          </div>
        </CardContent>
      </Card>

      {insights.length > 0 && (
        <div className="rounded-xl border-l-4 border-warning bg-warning/10 p-4">
          <div className="mb-2 flex items-center gap-2 text-sm font-semibold text-warning">
            <Info className="h-4 w-4" />
            {t('analyze.volumeInsightsTitle')}
          </div>
          <ul className="list-disc space-y-1 pl-5 text-xs text-text-1">
            {insights.map((s, i) => (
              <li key={i}>{s}</li>
            ))}
          </ul>
        </div>
      )}

      <Card>
        <CardContent className="flex flex-col gap-8 p-6 md:flex-row md:items-center">
          <PieChart slices={slices} total={total} />
          <div className="grid flex-1 grid-cols-1 gap-3 sm:grid-cols-2 xl:grid-cols-3">
            {slices.map((d) => (
              <div
                key={d.label}
                className={cn(
                  'flex items-center gap-2.5 rounded-lg border-l-4 bg-bg-1 px-3 py-2 text-sm transition-colors hover:bg-bg-2',
                )}
                style={{ borderLeftColor: d.color }}
              >
                <span className="flex-1 text-text-1">{d.label}</span>
                <span className="font-mono font-bold text-text-0">
                  {((d.size / (total || 1)) * 100).toFixed(1)}%
                </span>
              </div>
            ))}
          </div>
        </CardContent>
      </Card>

      <div className="grid gap-4 md:grid-cols-2">
        <Card>
          <CardContent className="p-5">
            <h3 className="mb-3 text-sm font-semibold text-text-2">
              {t('analyze.volumeAbiTitle')}
            </h3>
            {libAbis.length === 0 ? (
              <div className="text-xs text-text-2">—</div>
            ) : (
              <div className="overflow-hidden rounded-lg border border-border">
                <table className="w-full text-xs">
                  <tbody>
                    {libAbis.map(([abi, size]) => (
                      <tr key={abi} className="border-t border-border/60 first:border-t-0">
                        <td className="px-3 py-2 font-mono font-bold text-text-0">{abi}</td>
                        <td className="px-3 py-2 text-right text-text-1">{formatSize(size)}</td>
                        <td className="px-3 py-2 text-right text-text-2">
                          {((size / (stats.lib || 1)) * 100).toFixed(1)}%
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}
          </CardContent>
        </Card>

        <Card>
          <CardContent className="p-5">
            <h3 className="mb-3 text-sm font-semibold text-text-2">
              {t('analyze.volumeRedundantTitle')}
            </h3>
            {redundant.length === 0 ? (
              <div className="text-xs text-text-2">{t('analyze.volumeNoDuplicates')}</div>
            ) : (
              <div className="overflow-hidden rounded-lg border border-border">
                <table className="w-full text-xs">
                  <tbody>
                    {redundant.map((r) => {
                      const dupes = r.files.length - 1;
                      const first = r.files[0] ?? '';
                      const sample = first.split('/').pop() ?? first;
                      return (
                        <tr key={r.crc} className="border-t border-border/60 first:border-t-0 text-danger">
                          <td className="max-w-[260px] truncate px-3 py-2 font-mono" title={r.files.join('\n')}>
                            {sample}
                            <span className="ml-2 text-text-2">+{dupes} {t('analyze.volumeCopies')}</span>
                          </td>
                          <td className="px-3 py-2 text-right font-bold">
                            {formatSize(r.size * dupes)}
                          </td>
                        </tr>
                      );
                    })}
                  </tbody>
                </table>
              </div>
            )}
          </CardContent>
        </Card>
      </div>

      <Card>
        <CardContent className="p-5">
          <h3 className="mb-3 text-sm font-semibold text-text-2">
            {t('analyze.volumeLargestTitle')}
          </h3>
          {largest.length === 0 ? (
            <div className="text-xs text-text-2">—</div>
          ) : (
            <LargestFilesTable entries={largest} />
          )}
        </CardContent>
      </Card>
    </div>
  );
}
