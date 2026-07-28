import type { ApkInfo, VolumeStats, SecurityReport, VolumeEntry } from '@/ipc/analyze';
import type { ComponentHit, RuleReport } from '@/ipc/rules';

/**
 * Self-contained HTML report. No external assets, no dark mode — the
 * output is meant to be opened in a browser and printed/archived as a
 * single .html file. Styling mirrors the in-app dashboard but is
 * authored as static CSS so it works without Tauri / React.
 */

export type ReportPayload = {
  apkInfo: ApkInfo;
  ruleReport: RuleReport | null;
  generatedAt: string;
};

const COLORS = {
  dex: '#4caf50',
  lib: '#2196f3',
  res: '#ff9800',
  assets: '#9c27b0',
  manifest: '#607d8b',
  arsc: '#f44336',
  other: '#9e9e9e',
};

function formatSize(bytes: number | null | undefined): string {
  if (!bytes) return '0 B';
  const k = 1024;
  const units = ['B', 'KB', 'MB', 'GB'];
  const i = Math.min(Math.floor(Math.log(bytes) / Math.log(k)), units.length - 1);
  const v = bytes / Math.pow(k, i);
  return `${v.toFixed(2)} ${units[i]}`;
}

function escapeHtml(s: string | null | undefined): string {
  if (s == null) return '—';
  return s
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}

function scoreColor(score: number): string {
  if (score >= 85) return '#22c55e';
  if (score >= 60) return '#f59e0b';
  return '#ef4444';
}

function buildSlices(stats: VolumeStats): { label: string; size: number; color: string }[] {
  const keys: Array<['dex' | 'lib' | 'res' | 'assets' | 'manifest' | 'arsc' | 'other', string]> = [
    ['dex', 'Dex'],
    ['lib', 'Libs'],
    ['res', 'Res'],
    ['assets', 'Assets'],
    ['arsc', 'Arsc'],
    ['manifest', 'Manifest'],
    ['other', 'Other'],
  ];
  return keys
    .map(([k, label]) => ({ label, size: stats[k], color: COLORS[k] }))
    .filter((d) => d.size > 0)
    .sort((a, b) => b.size - a.size);
}

function renderPieSvg(
  slices: { size: number; color: string }[],
  total: number,
): string {
  if (slices.length === 0 || total === 0) {
    return `<svg viewBox="0 0 180 180" width="180" height="180"><circle cx="90" cy="90" r="80" fill="#eef2f7"/></svg>`;
  }
  const r = 80;
  const cx = 90;
  const cy = 90;
  let cumulative = 0;
  const paths = slices.map((d) => {
    const start = (cumulative / total) * 2 * Math.PI;
    cumulative += d.size;
    const end = (cumulative / total) * 2 * Math.PI;
    const x1 = cx + r * Math.sin(start);
    const y1 = cy - r * Math.cos(start);
    const x2 = cx + r * Math.sin(end);
    const y2 = cy - r * Math.cos(end);
    const largeArc = d.size / total > 0.5 ? 1 : 0;
    return `<path d="M ${cx} ${cy} L ${x1.toFixed(2)} ${y1.toFixed(2)} A ${r} ${r} 0 ${largeArc} 1 ${x2.toFixed(2)} ${y2.toFixed(2)} Z" fill="${d.color}" stroke="#fff" stroke-width="2"/>`;
  });
  return `<svg viewBox="0 0 180 180" width="180" height="180">${paths.join('')}</svg>`;
}

function renderScoreSvg(score: number): string {
  const color = scoreColor(score);
  return `
    <svg viewBox="0 0 36 36" width="120" height="120" style="transform: rotate(-90deg);">
      <path d="M18 2.0845 a 15.9155 15.9155 0 0 1 0 31.831 a 15.9155 15.9155 0 0 1 0 -31.831"
        fill="none" stroke="rgba(15,23,42,0.08)" stroke-width="3"/>
      <path d="M18 2.0845 a 15.9155 15.9155 0 0 1 0 31.831 a 15.9155 15.9155 0 0 1 0 -31.831"
        fill="none" stroke="${color}" stroke-width="3" stroke-dasharray="${score}, 100"/>
    </svg>
    <div style="position:absolute; top:50%; left:50%; transform:translate(-50%,-50%); text-align:center;">
      <span style="display:block; font-size:28px; font-weight:800; color:${color}; line-height:1;">${score}</span>
      <span style="font-size:10px; color:#94a3b8;">Score</span>
    </div>
  `;
}

function renderComponentTable(title: string, items: string[]): string {
  if (items.length === 0) {
    return `<div class="component-block">
      <div class="component-title">${escapeHtml(title)} <span class="count">0</span></div>
      <div class="empty">—</div>
    </div>`;
  }
  return `<div class="component-block">
    <div class="component-title">${escapeHtml(title)} <span class="count">${items.length}</span></div>
    <div class="component-list">${items.map((i) => `<div class="row">${escapeHtml(i)}</div>`).join('')}</div>
  </div>`;
}

function renderComponentSection(title: string, hits: ComponentHit[]): string {
  if (hits.length === 0) return '';
  const rows = hits
    .map((h) => {
      const m = h.matched_rule;
      if (!m) {
        return `<li class="component-row">
          <span class="mono">${escapeHtml(h.name)}</span>
          <span class="muted small">no match</span>
        </li>`;
      }
      const sev = String(m.severity).toLowerCase();
      const cls =
        sev === 'danger' || sev === 'critical'
          ? 'sev-danger'
          : sev === 'warn' || sev === 'warning'
          ? 'sev-warn'
          : 'sev-info';
      const meta = (m.metadata || {}) as Record<string, string | undefined>;
      const label = meta.label || '';
      const dev = meta.dev_team || '';
      const link = meta.source_link || '';
      const zh = meta.zh_description || '';
      return `<li class="component-row">
        <span class="mono">${escapeHtml(h.name)}</span>
        <span class="rule-sev ${cls}">${escapeHtml(String(m.severity))}</span>
        <span class="rule-title">${escapeHtml(m.description)}</span>
        ${label ? `<span class="rule-label">${escapeHtml(label)}</span>` : ''}
      </li>
      ${
        zh
          ? `<li class="rule-detail"><b>ZH:</b> ${escapeHtml(zh)}</li>`
          : ''
      }
      ${
        dev
          ? `<li class="rule-detail"><b>Dev:</b> ${escapeHtml(dev)}</li>`
          : ''
      }
      ${
        link
          ? `<li class="rule-detail"><a href="${escapeHtml(link)}" target="_blank" rel="noreferrer">${escapeHtml(link)}</a></li>`
          : ''
      }`;
    })
    .join('');
  return `<h3>${escapeHtml(title)} <span class="muted small">${hits.length}</span></h3>
    <ul class="components">${rows}</ul>`;
}

function renderDashboard(payload: ReportPayload): string {
  const { apkInfo } = payload;
  const compCount =
    apkInfo.activities.length + apkInfo.services.length + apkInfo.receivers.length + apkInfo.providers.length;
  const matchedCount = payload.ruleReport ? payload.ruleReport.total_matched : 0;
  const total = apkInfo.volume_total_size ?? 0;
  const score = apkInfo.security_report?.score ?? null;
  const waste = apkInfo.volume_stats?.waste_size ?? 0;

  const cells: { label: string; value: string; unit?: string | undefined; desc: string }[] = [
    { label: 'BASIC INFO', value: '7', unit: 'fields', desc: 'Decoded attributes' },
    { label: 'PERMISSIONS', value: String(apkInfo.permissions.length), unit: 'perms', desc: 'Declared in manifest' },
    { label: 'COMPONENTS', value: String(compCount), unit: 'items', desc: 'Activities / services / receivers / providers' },
    { label: 'RULE HITS', value: String(matchedCount), desc: 'Findings across installed rule packs' },
    {
      label: 'VOLUME',
      value: formatSize(total),
      desc: waste > 0 ? `Includes ${formatSize(waste)} of redundant content` : 'No redundant content',
    },
    {
      label: 'SECURITY',
      value: score != null ? String(score) : '—',
      unit: score != null ? '/ 100' : undefined,
      desc: score != null ? (score >= 85 ? 'All clear' : `${apkInfo.security_report?.risks.length ?? 0} findings`) : 'Not scanned',
    },
  ];
  return `<div class="dashboard">${cells
    .map(
      (c) => `<div class="dash-card">
        <div class="dash-label">${escapeHtml(c.label)}</div>
        <div class="dash-value">${escapeHtml(c.value)}${c.unit ? ` <span class="dash-unit">${escapeHtml(c.unit)}</span>` : ''}</div>
        <div class="dash-desc">${escapeHtml(c.desc)}</div>
      </div>`,
    )
    .join('')}</div>`;
}

function renderHeader(payload: ReportPayload): string {
  const { apkInfo } = payload;
  const label = apkInfo.application_label?.trim() || apkInfo.package_name;
  const version = apkInfo.version_name || apkInfo.version_code
    ? `v${apkInfo.version_name ?? '?'} (${apkInfo.version_code ?? '?'})`
    : '—';
  const initial = (label || '?').trim().charAt(0).toUpperCase() || '?';
  const sdk = apkInfo.min_sdk || apkInfo.target_sdk
    ? `Min ${apkInfo.min_sdk ?? '?'} / Target ${apkInfo.target_sdk ?? '?'}${apkInfo.max_sdk ? ` / Max ${apkInfo.max_sdk}` : ''}`
    : '—';
  const tech = (apkInfo.tech_stack ?? [])
    .map((s) => `<span class="tech">${escapeHtml(s)}</span>`)
    .join('');
  return `<div class="header">
    <div class="app-icon">${escapeHtml(initial)}</div>
    <div class="app-info">
      <h1>${escapeHtml(label)}</h1>
      <div class="badges">
        <span class="badge primary">${escapeHtml(apkInfo.package_name)}</span>
        <span class="badge">${escapeHtml(version)}</span>
      </div>
      <div class="meta">SDK <span class="mono">${escapeHtml(sdk)}</span></div>
      ${tech ? `<div class="tech-row">${tech}</div>` : ''}
    </div>
  </div>`;
}

function renderVolume(payload: ReportPayload): string {
  const { apkInfo } = payload;
  const stats = apkInfo.volume_stats;
  if (!stats) return '<div class="empty">No volume data.</div>';
  const total = apkInfo.volume_total_size ?? 0;
  const slices = buildSlices(stats);
  const libAbis = Object.entries(stats.lib_breakdown || {});
  const redundant = (stats.redundant_files || []).slice(0, 5);
  const largest = (apkInfo.largest_files || []).slice(0, 10);
  const insights = apkInfo.insights || [];

  const insightsHtml = insights.length
    ? `<div class="insights"><h4>Size Diagnosis</h4><ul>${insights.map((s) => `<li>${escapeHtml(s)}</li>`).join('')}</ul></div>`
    : '';

  const largestRows = largest
    .map(
      (f: VolumeEntry) => `<tr>
        <td class="mono path" title="${escapeHtml(f.name)}">${escapeHtml(f.name)}</td>
        <td class="num">${formatSize(f.size)}</td>
        <td><div class="bar"><div class="bar-fill" style="width:${Math.min(100, f.ratio).toFixed(1)}%"></div></div></td>
        <td class="num small">${f.ratio.toFixed(1)}%</td>
      </tr>`,
    )
    .join('');

  return `
    <section class="block">
      <h2>Volume</h2>
      <div class="volume-header">
        <h3>Total Size (Compressed)</h3>
        <div class="total">${formatSize(total)}${stats.waste_size > 0 ? `<div class="waste">Waste: ${formatSize(stats.waste_size)} (Redundant)</div>` : ''}</div>
      </div>
      ${insightsHtml}
      <div class="volume-chart">
        <div class="pie">${renderPieSvg(slices, total)}</div>
        <div class="legend">
          ${slices
            .map(
              (d) => `<div class="legend-item" style="border-left-color:${d.color}">
                <span>${escapeHtml(d.label)}</span>
                <span class="mono">${((d.size / (total || 1)) * 100).toFixed(1)}%</span>
              </div>`,
            )
            .join('')}
        </div>
      </div>
      <div class="two-col">
        <div>
          <h3>Native Libs by ABI</h3>
          <table class="kv">
            ${libAbis.length
              ? libAbis
                  .map(
                    ([abi, size]) => `<tr>
                      <td class="mono strong">${escapeHtml(abi)}</td>
                      <td class="num">${formatSize(size)}</td>
                      <td class="num muted">${((size / (stats.lib || 1)) * 100).toFixed(1)}%</td>
                    </tr>`,
                  )
                  .join('')
              : '<tr><td class="muted" colspan="3">—</td></tr>'}
          </table>
        </div>
        <div>
          <h3>Top Waste (Duplicate Files)</h3>
          <table class="kv">
            ${redundant.length
              ? redundant
                  .map((r) => {
                    const dupes = r.files.length - 1;
                    const sample = (r.files[0] ?? '').split('/').pop() ?? '';
                    return `<tr class="danger">
                      <td class="mono" title="${escapeHtml(r.files.join('\n'))}">${escapeHtml(sample)} <span class="muted">+${dupes} copies</span></td>
                      <td class="num strong">${formatSize(r.size * dupes)}</td>
                    </tr>`;
                  })
                  .join('')
              : '<tr><td class="muted" colspan="2">No duplicates found</td></tr>'}
          </table>
        </div>
      </div>
      <h3>Largest Files</h3>
      <table class="kv">${largestRows || '<tr><td class="muted" colspan="4">—</td></tr>'}</table>
    </section>
  `;
}

function renderSecurity(payload: ReportPayload): string {
  const report: SecurityReport | null | undefined = payload.apkInfo.security_report;
  if (!report) {
    return `<section class="block"><h2>Security</h2><div class="empty">Not scanned.</div></section>`;
  }
  const sorted = [...report.risks].sort((a, b) => {
    const order: Record<string, number> = { critical: 0, warning: 1, info: 2 };
    return (order[a.level] ?? 9) - (order[b.level] ?? 9);
  });
  return `
    <section class="block">
      <h2>Security</h2>
      <div class="security-overview">
        <div class="score-wrap">${renderScoreSvg(report.score)}</div>
        <div class="security-text">
          <h3>APK Security Diagnostic Report</h3>
          <p>${report.risks.length === 0 ? 'No issues detected.' : `This scan surfaced ${report.risks.length} potential risk(s).`}</p>
        </div>
      </div>
      <div class="risks">
        ${sorted
          .map(
            (r) => `<div class="risk risk-${escapeHtml(r.level)}">
              <div class="risk-header">
                <span class="rule-sev sev-${escapeHtml(r.level)}">${escapeHtml(r.level)}</span>
                <strong>${escapeHtml(r.title)}</strong>
              </div>
              <p>${escapeHtml(r.description)}</p>
              <div class="risk-suggest"><b>Suggestion:</b> ${escapeHtml(r.suggestion)}</div>
            </div>`,
          )
          .join('')}
      </div>
    </section>
  `;
}

function renderRules(payload: ReportPayload): string {
  if (!payload.ruleReport) return '<div class="empty">No rule report.</div>';
  const c = payload.ruleReport.components;
  const total = payload.ruleReport.total_matched;
  if (total === 0) return '<div class="empty">No rule hits.</div>';
  return `<section class="block">
    <h2>Rule Report <span class="muted small">${total} matched</span></h2>
    ${renderComponentSection('Native Libraries', c.native_libraries)}
    ${renderComponentSection('Activities', c.activities)}
    ${renderComponentSection('Services', c.services)}
    ${renderComponentSection('Receivers', c.receivers)}
    ${renderComponentSection('Providers', c.providers)}
  </section>`;
}

function renderOverview(payload: ReportPayload): string {
  const i = payload.apkInfo;
  return `
    <section class="block">
      <h2>Overview</h2>
      <table class="kv">
        <tr><td class="k">Package</td><td class="mono">${escapeHtml(i.package_name)}</td></tr>
        <tr><td class="k">App Name</td><td>${escapeHtml(i.application_label)}</td></tr>
        <tr><td class="k">Version Name</td><td>${escapeHtml(i.version_name)}</td></tr>
        <tr><td class="k">Version Code</td><td>${escapeHtml(i.version_code)}</td></tr>
        <tr><td class="k">minSdk</td><td>${escapeHtml(i.min_sdk)}</td></tr>
        <tr><td class="k">targetSdk</td><td>${escapeHtml(i.target_sdk)}</td></tr>
        <tr><td class="k">maxSdk</td><td>${escapeHtml(i.max_sdk)}</td></tr>
      </table>
    </section>
  `;
}

function renderPermissions(payload: ReportPayload): string {
  const perms = payload.apkInfo.permissions;
  if (perms.length === 0) return '<div class="empty">No permissions declared.</div>';
  return `<div class="perm-grid">${perms.map((p) => `<span class="badge">${escapeHtml(p)}</span>`).join('')}</div>`;
}

function renderComponents(payload: ReportPayload): string {
  const i = payload.apkInfo;
  return `<div class="components-grid">
    ${renderComponentTable('Activity', i.activities)}
    ${renderComponentTable('Service', i.services)}
    ${renderComponentTable('Receiver', i.receivers)}
    ${renderComponentTable('Provider', i.providers)}
  </div>`;
}

const REPORT_CSS = `
:root { color-scheme: light; }
* { box-sizing: border-box; }
body { font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif; background: #f6f8fb; color: #0f172a; margin: 0; padding: 32px; line-height: 1.55; }
.container { max-width: 1000px; margin: 0 auto; }
.header { display: flex; align-items: flex-start; gap: 20px; background: #fff; border: 1px solid rgba(15,23,42,0.1); border-radius: 12px; padding: 24px; box-shadow: 0 4px 12px rgba(15,23,42,0.06); position: relative; }
.header::before { content: ''; position: absolute; inset: 0; background: linear-gradient(135deg, rgba(37,99,235,0.08), transparent 60%, rgba(245,158,11,0.08)); border-radius: 12px; pointer-events: none; }
.app-icon { position: relative; width: 80px; height: 80px; border-radius: 16px; background: linear-gradient(135deg, rgba(37,99,235,0.25), transparent 70%); display: flex; align-items: center; justify-content: center; font-size: 32px; font-weight: 800; color: #1d4ed8; border: 1px solid rgba(15,23,42,0.1); flex-shrink: 0; }
.app-info { position: relative; flex: 1; min-width: 0; }
.app-info h1 { margin: 0 0 6px 0; font-size: 24px; font-weight: 700; }
.badges { display: flex; flex-wrap: wrap; gap: 6px; margin-bottom: 8px; }
.badge { display: inline-block; padding: 2px 8px; border-radius: 4px; font-size: 11px; font-weight: 600; background: #fff; border: 1px solid rgba(15,23,42,0.12); color: #475569; }
.badge.primary { background: #2563eb; color: #fff; border-color: #2563eb; font-family: ui-monospace, SFMono-Regular, monospace; }
.meta { font-size: 12px; color: #64748b; }
.meta .mono { font-family: ui-monospace, SFMono-Regular, monospace; color: #0f172a; }
.tech-row { display: flex; flex-wrap: wrap; gap: 6px; margin-top: 8px; }
.tech { font-family: ui-monospace, SFMono-Regular, monospace; font-size: 11px; padding: 2px 8px; border-radius: 4px; background: #eef2f7; color: #475569; }

.dashboard { display: grid; grid-template-columns: repeat(auto-fit, minmax(220px, 1fr)); gap: 12px; margin: 24px 0; }
.dash-card { background: #fff; border: 1px solid rgba(15,23,42,0.08); border-radius: 12px; padding: 16px; position: relative; overflow: hidden; }
.dash-card::before { content: ''; position: absolute; inset: 0; background: linear-gradient(135deg, rgba(37,99,235,0.04), transparent 60%); pointer-events: none; }
.dash-label { font-size: 11px; font-weight: 600; color: #94a3b8; text-transform: uppercase; letter-spacing: 0.5px; }
.dash-value { font-size: 22px; font-weight: 800; color: #0f172a; line-height: 1.1; margin-top: 4px; }
.dash-unit { font-size: 12px; font-weight: 500; color: #64748b; }
.dash-desc { font-size: 11px; color: #64748b; margin-top: 4px; }

.block { background: #fff; border: 1px solid rgba(15,23,42,0.08); border-radius: 12px; padding: 24px; margin-bottom: 24px; box-shadow: 0 2px 6px rgba(15,23,42,0.04); }
.block h2 { font-size: 18px; font-weight: 700; margin: 0 0 16px 0; color: #0f172a; border-bottom: 1px solid rgba(15,23,42,0.08); padding-bottom: 10px; }
.block h3 { font-size: 14px; font-weight: 600; margin: 16px 0 8px 0; color: #475569; }

table.kv { width: 100%; border-collapse: separate; border-spacing: 0 4px; font-size: 13px; }
table.kv td { padding: 8px 12px; background: rgba(15,23,42,0.02); }
table.kv td.k { width: 140px; color: #64748b; font-size: 12px; }
table.kv td.mono { font-family: ui-monospace, SFMono-Regular, monospace; }
table.kv td.muted { color: #94a3b8; }
table.kv td.strong { font-weight: 700; }
table.kv td.num { text-align: right; }
table.kv td.danger { color: #ef4444; }
table.kv td.path { max-width: 400px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
table.kv td.small { font-size: 10px; font-weight: 700; width: 60px; }

.bar { height: 6px; background: rgba(15,23,42,0.05); border-radius: 3px; overflow: hidden; }
.bar-fill { height: 100%; background: #2563eb; border-radius: 3px; }

.volume-header { display: flex; align-items: center; justify-content: space-between; }
.volume-header .total { font-size: 22px; font-weight: 800; color: #2563eb; }
.volume-header .waste { font-size: 11px; color: #ef4444; margin-top: 2px; }

.insights { background: rgba(245,158,11,0.08); border-left: 4px solid #f59e0b; border-radius: 6px; padding: 12px 16px; margin: 16px 0; }
.insights h4 { margin: 0 0 6px 0; font-size: 13px; color: #b45309; }
.insights ul { margin: 0; padding-left: 20px; font-size: 12px; color: #1e293b; }

.volume-chart { display: flex; align-items: center; gap: 32px; margin: 24px 0; padding: 24px; background: rgba(15,23,42,0.02); border-radius: 12px; }
.pie { flex-shrink: 0; }
.legend { flex: 1; display: grid; grid-template-columns: repeat(auto-fill, minmax(140px, 1fr)); gap: 8px; }
.legend-item { display: flex; align-items: center; justify-content: space-between; gap: 8px; font-size: 13px; padding: 8px 12px; background: #fff; border-radius: 6px; border-left: 4px solid; }

.two-col { display: grid; grid-template-columns: 1fr 1fr; gap: 24px; margin: 16px 0; }

.security-overview { display: flex; align-items: center; gap: 32px; padding: 20px; background: rgba(15,23,42,0.02); border-radius: 12px; margin-bottom: 16px; }
.score-wrap { position: relative; width: 120px; height: 120px; flex-shrink: 0; }
.security-text h3 { margin: 0 0 4px 0; font-size: 16px; }
.security-text p { margin: 0; font-size: 13px; color: #64748b; }
.risks { display: flex; flex-direction: column; }
.risk { margin-bottom: 12px; padding: 14px 16px; border-radius: 8px; background: rgba(15,23,42,0.02); border-left: 4px solid; }
.risk-critical { border-left-color: #ef4444; }
.risk-warning { border-left-color: #f59e0b; }
.risk-info { border-left-color: #3b82f6; }
.risk-header { display: flex; align-items: center; gap: 8px; margin-bottom: 8px; }
.risk p { margin: 0 0 8px 0; font-size: 13px; color: #334155; }
.risk-suggest { font-size: 12px; padding: 8px 10px; background: rgba(15,23,42,0.05); border-radius: 4px; color: #1e293b; }
.risk-suggest b { color: #0f172a; }
.rule-sev { display: inline-block; padding: 1px 6px; border-radius: 3px; font-size: 10px; font-weight: 700; text-transform: uppercase; }
.sev-critical, .sev-danger { background: #ef444422; color: #ef4444; }
.sev-warning, .sev-warn { background: #f59e0b22; color: #f59e0b; }
.sev-info { background: #3b82f622; color: #3b82f6; }

.perm-grid { display: flex; flex-wrap: wrap; gap: 6px; }
.components-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; }
.component-block { border: 1px solid rgba(15,23,42,0.08); border-radius: 8px; padding: 12px; background: rgba(15,23,42,0.02); }
.component-title { font-size: 13px; font-weight: 600; margin-bottom: 6px; display: flex; justify-content: space-between; align-items: center; }
.component-title .count { font-size: 11px; background: #2563eb; color: #fff; padding: 1px 6px; border-radius: 3px; }
.component-list { max-height: 220px; overflow: auto; font-family: ui-monospace, SFMono-Regular, monospace; font-size: 12px; }
.component-list .row { padding: 4px 8px; border-radius: 3px; }
.component-list .row:hover { background: rgba(15,23,42,0.04); }
.empty { text-align: center; color: #94a3b8; padding: 24px; font-size: 13px; }

.rule { margin-bottom: 10px; padding: 12px; border-radius: 8px; background: rgba(15,23,42,0.02); border-left: 4px solid; }
.rule.sev-danger, .rule.sev-critical { border-left-color: #ef4444; }
.rule.sev-warn, .rule.sev-warning { border-left-color: #f59e0b; }
.rule.sev-info { border-left-color: #3b82f6; }
.rule-header { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
.rule-title { font-size: 13px; font-weight: 600; }
.rule-label { font-size: 10px; padding: 1px 6px; background: #eef2f7; border-radius: 3px; color: #475569; }
.rule-evidence { font-family: ui-monospace, SFMono-Regular, monospace; font-size: 11px; color: #475569; margin-top: 4px; }
.rule-meta { font-size: 11px; color: #64748b; margin-top: 2px; }
.rule-meta a { color: #2563eb; text-decoration: none; }

.footer { text-align: center; font-size: 11px; color: #94a3b8; padding: 24px 0; }
`;

export function buildReportHtml(payload: ReportPayload): string {
  return `<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>APK Analysis Report — ${escapeHtml(payload.apkInfo.package_name)}</title>
<style>${REPORT_CSS}</style>
</head>
<body>
<div class="container">
  ${renderHeader(payload)}
  ${renderDashboard(payload)}
  ${renderOverview(payload)}
  <section class="block"><h2>Permissions (${payload.apkInfo.permissions.length})</h2>${renderPermissions(payload)}</section>
  <section class="block"><h2>Components</h2>${renderComponents(payload)}</section>
  ${renderVolume(payload)}
  ${renderSecurity(payload)}
  ${renderRules(payload)}
  <div class="footer">Generated by JADB at ${escapeHtml(payload.generatedAt)}</div>
</div>
</body>
</html>`;
}
