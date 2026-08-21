import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { CheckCircle2, Copy, Cpu, Crown, KeyRound, Loader2, RefreshCw, Trash2, Wifi, WifiOff } from 'lucide-react';
import { toast } from 'sonner';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Input } from '@/components/ui/input';
import { Textarea } from '@/components/ui/textarea';
import { useLicenseStore } from '@/store/license';
import { updateSettings } from '@/ipc/useTauri';

function fmtTime(iso: string | null): string {
  if (!iso) return '-';
  const d = new Date(iso);
  if (isNaN(d.getTime())) return iso;
  return d.toLocaleString();
}

export function LicenseTab() {
  const { t, i18n } = useTranslation();
  const { status, loading, error, serverUrl, activate, remove, refreshRemote, replaceBinding, loadServerUrl } = useLicenseStore();
  const [token, setToken] = useState('');
  const [serverUrlDraft, setServerUrlDraft] = useState('');
  const [savingUrl, setSavingUrl] = useState(false);
  const active = status?.state === 'active';

  useEffect(() => {
    void loadServerUrl();
  }, [loadServerUrl]);

  useEffect(() => {
    setServerUrlDraft(serverUrl ?? '');
  }, [serverUrl]);

  async function submit() {
    if (!token.trim()) return;
    try {
      await activate(token);
      setToken('');
      toast.success(t('license.activated'));
    } catch (e) {
      toast.error(String(e));
    }
  }

  async function copyDevice() {
    if (!status?.deviceId) return;
    await navigator.clipboard.writeText(status.deviceId);
    toast.success(t('license.deviceCopied'));
  }

  async function onReplaceBinding() {
    const bound = status?.boundDeviceId;
    const msg = bound
      ? `确认替换绑定？当前 license 已绑定到机器：
${bound}

替换后该机器的授权将立即失效，本机将获得授权。`
      : '确认替换绑定？';
    if (!window.confirm(msg)) return;
    try {
      await replaceBinding();
      toast.success('替换绑定成功，本机已激活');
    } catch (e) {
      toast.error(`替换失败：${String(e)}`);
    }
  }

  async function saveServerUrl() {
    const value = serverUrlDraft.trim();
    // 前端先校验：URL 必须 http(s) 开头；空字符串 = 关闭（合法）。
    // 这是为了防止「把 license token 误粘到 URL 框」这种常见错误。
    if (value && !/^https?:\/\//i.test(value)) {
      toast.error(`这不是一个合法的 URL：应以 http:// 或 https:// 开头。\n\n你粘贴的可能是 license token，请到下方「激活 VIP」框粘贴。`);
      return;
    }
    setSavingUrl(true);
    try {
      await updateSettings({ licenseServerUrl: value === '' ? null : value });
      await loadServerUrl();
      toast.success(value ? `已启用在线激活：${value}` : '已关闭在线激活，回到纯离线模式');
      // 触发一次 status 刷新，让 mode 重新计算
      await useLicenseStore.getState().refresh();
    } catch (e) {
      toast.error(`保存失败：${String(e)}`);
    } finally {
      setSavingUrl(false);
    }
  }

  async function onRefresh() {
    if (!serverUrl) {
      toast.error('未配置 license server URL，无法在线同步');
      return;
    }
    try {
      await refreshRemote();
      toast.success('已同步 license-server 状态');
    } catch (e) {
      toast.error(`同步失败：${String(e)}`);
    }
  }

  const mode = status?.mode ?? 'offline';
  const lastVerified = status?.lastVerifiedAt ?? null;

  return (
    <div className="space-y-4">
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Crown className="h-5 w-5 text-warning" />
            {t('license.statusTitle')}
          </CardTitle>
        </CardHeader>
        <CardContent className="space-y-3 text-sm">
          <div className="flex items-center gap-2">
            <Badge variant={active ? 'success' : status?.state === 'expired' ? 'danger' : 'secondary'}>
              {active ? t('license.vip') : t(`license.state.${status?.state ?? 'unlicensed'}`)}
            </Badge>
            {status?.licensedTo && <span>{status.licensedTo}</span>}
          </div>

          {/* 盲 license + 无 server URL：提示用户去配置 server */}
          {status?.state === 'noDeviceBinding' && (
            <div className="rounded-md border border-warning/40 bg-warning/10 px-3 py-2 text-xs">
              <div className="flex flex-wrap items-center justify-between gap-2">
                <span className="text-text-1">{t('license.noDeviceBindingHint')}</span>
                {!serverUrl && (
                  <Button
                    size="sm"
                    variant="outline"
                    onClick={() => {
                      const el = document.querySelector<HTMLInputElement>(
                        'input[placeholder="http://your-server:8787   （注意：不是 license token）"]'
                      );
                      el?.focus();
                      el?.scrollIntoView({ behavior: 'smooth', block: 'center' });
                    }}
                  >
                    去配置
                  </Button>
                )}
              </div>
            </div>
          )}

          {/* 替换绑定提示：在线模式下 license 已绑到其他机器时显示 */}
          {status?.state === 'deviceMismatch' && status?.canRebind && (
            <div className="rounded-md border border-warning/40 bg-warning/10 px-3 py-2 text-xs">
              <div className="flex flex-wrap items-center justify-between gap-2">
                <span className="text-text-1">
                  {t('license.boundToOther')}{' '}
                  <code className="font-mono text-text-0">
                    {status.boundDeviceId ?? '?'}
                  </code>
                  。
                </span>
                <Button
                  size="sm"
                  variant="outline"
                  onClick={() => void onReplaceBinding()}
                  disabled={loading}
                >
                  {t('license.replaceBinding')}
                </Button>
              </div>
            </div>
          )}

          {/* 模式徽章 + 最后校验时间 */}
          <div className="flex flex-wrap items-center gap-2 text-xs text-text-2">
            {mode === 'online' && (
              <span className="inline-flex items-center gap-1 rounded-md bg-success/15 px-2 py-0.5 text-success">
                <Wifi className="h-3 w-3" /> 在线激活
              </span>
            )}
            {mode === 'offline_degraded' && (
              <span className="inline-flex items-center gap-1 rounded-md bg-warning/15 px-2 py-0.5 text-warning">
                <WifiOff className="h-3 w-3" /> 离线降级（服务器不可达）
              </span>
            )}
            {mode === 'offline' && (
              <span className="inline-flex items-center gap-1 rounded-md bg-bg-2 px-2 py-0.5 text-text-2">
                纯离线
              </span>
            )}
            {lastVerified && (
              <span className="text-text-2">
                · 最后同步：<span className="text-text-1">{fmtTime(lastVerified)}</span>
              </span>
            )}
            {serverUrl && (
              <button
                onClick={() => void onRefresh()}
                disabled={loading}
                className="ml-auto inline-flex items-center gap-1 rounded-md border border-border px-2 py-0.5 text-text-1 hover:bg-bg-2 disabled:opacity-50"
              >
                {loading ? <Loader2 className="h-3 w-3 animate-spin" /> : <RefreshCw className="h-3 w-3" />}
                立即同步
              </button>
            )}
          </div>

          {active && (
            <div>
              {status.perpetual
                ? t('license.perpetual')
                : t('license.expiresAt', { date: status.expiresAt ? new Date(status.expiresAt).toLocaleDateString(i18n.language) : '-' })}
            </div>
          )}
          {active && status.features.map((feature) => (
            <div key={feature} className="flex gap-2">
              <CheckCircle2 className="h-4 w-4 text-success" />
              {t(`license.feature.${feature}`)}
            </div>
          ))}

          {!active && (
            <div className="space-y-3 pt-1">
              <p className="text-xs font-medium text-text-2">{t('license.vipBenefits')}</p>
              <div className="grid gap-2 sm:grid-cols-2">
                <div className="flex items-center gap-2 text-text-1">
                  <CheckCircle2 className="h-4 w-4 text-brand" />{t('license.feature.apk_report_export')}
                </div>
                <div className="flex items-center gap-2 text-text-1">
                  <CheckCircle2 className="h-4 w-4 text-brand" />{t('license.feature.signing_v31')}
                </div>
                <div className="flex items-center gap-2 text-text-1">
                  <CheckCircle2 className="h-4 w-4 text-brand" />{t('license.feature.adb_multi_device')}
                </div>
                <div className="flex items-center gap-2 text-text-1">
                  <CheckCircle2 className="h-4 w-4 text-brand" />{t('license.feature.adb_batch_install')}
                </div>
              </div>
              <div className="flex flex-wrap items-center justify-between gap-2 border-t border-border pt-3 text-text-2">
                <span>{t('license.purchasePrefix')} <code className="font-semibold text-text-0">godfeer</code></span>
                <Button size="sm" variant="ghost" onClick={() => void navigator.clipboard.writeText('godfeer').then(() => toast.success(t('license.wechatCopied')))}>
                  <Copy className="h-3.5 w-3.5" />{t('license.copyWechat')}
                </Button>
              </div>
            </div>
          )}

          {status?.message && <p className="text-danger">{status.message}</p>}
          {error && <p className="text-danger">{error}</p>}
          {active && (
            <Button variant="outline" onClick={() => void remove()} disabled={loading}>
              <Trash2 className="h-4 w-4" />{t('license.remove')}
            </Button>
          )}
        </CardContent>
      </Card>

      {/* —— 激活 VIP（未激活时置顶，紧贴状态卡）—— */}
      {!active && (
        <Card>
          <CardHeader>
            <CardTitle className="flex items-center gap-2">
              <KeyRound className="h-4 w-4 text-warning" />
              {t('license.activateTitle')}
            </CardTitle>
          </CardHeader>
          <CardContent className="space-y-3">
            <p className="text-sm text-text-2">
              {t('license.activateHint')}
              <br />
              <span className="text-xs text-muted2">
                如果 token 里嵌了 server URL，激活时会自动配置；不需要手动填 URL。
              </span>
            </p>
            <Textarea
              value={token}
              onChange={(e) => setToken(e.target.value)}
              placeholder="JADB1.xxx.yyy"
              className="min-h-28 font-mono"
            />
            <div className="flex gap-2">
              <Button onClick={() => void submit()} disabled={loading || !token.trim()}>
                {loading && <Loader2 className="h-4 w-4 animate-spin" />}
                {t('license.activate')}
              </Button>
              {serverUrl && (
                <span className="flex items-center gap-1 text-xs text-text-2">
                  <Wifi className="h-3 w-3 text-success" />
                  当前在线激活
                </span>
              )}
            </div>
          </CardContent>
        </Card>
      )}

      {/* —— 本机设备码（激活时给管理员用） —— */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2 text-base">
            <Cpu className="h-4 w-4" />
            {t('license.deviceTitle')}
          </CardTitle>
        </CardHeader>
        <CardContent className="space-y-3">
          <p className="text-sm text-text-2">{t('license.deviceHint')}</p>
          <div className="flex gap-2">
            <code className="flex-1 rounded-md bg-bg-2 px-3 py-2 text-sm">{status?.deviceId ?? '-'}</code>
            <Button variant="outline" onClick={() => void copyDevice()}>
              <Copy className="h-4 w-4" />{t('license.copy')}
            </Button>
          </div>
        </CardContent>
      </Card>

      {/* —— 高级：手动配置 License Server URL（默认折叠）
         注意：粘贴 token 时，如果 token 嵌了 license_server_url，会自动写到 settings 里。
         大多数用户不需要看这一段，只在以下场景用：
         - 老 token（没有嵌入 URL）+ 临时想切换验证服务器
         - 调试 / 验证 server 联通性
       */}
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2 text-sm text-text-1">
            {serverUrl ? <Wifi className="h-4 w-4 text-success" /> : <WifiOff className="h-4 w-4 text-text-2" />}
            高级：License Server（在线激活）
          </CardTitle>
        </CardHeader>
        <CardContent className="space-y-3 text-sm">
          <details>
            <summary className="cursor-pointer text-xs text-text-2 hover:text-text-1 select-none">
              {serverUrl ? `当前：${serverUrl}` : '点击展开（默认不需要配置）'}
            </summary>
            <div className="mt-3 space-y-3">
              <p className="text-xs text-text-2">
                粘贴 token 时如果 token 里嵌入了 server URL，会自动写到此处。
                服务器不可达时会自动降级到本地离线校验。
              </p>
              <div className="flex gap-2">
                <Input
                  value={serverUrlDraft}
                  onChange={(e) => setServerUrlDraft(e.target.value)}
                  placeholder="http://your-server:8787"
                  className="font-mono text-xs"
                />
                <Button onClick={() => void saveServerUrl()} disabled={savingUrl}>
                  {savingUrl && <Loader2 className="h-4 w-4 animate-spin" />}
                  保存
                </Button>
              </div>
              <p className="text-xs text-text-2">
                留空表示关闭在线激活，回到纯离线模式。
              </p>
            </div>
          </details>
        </CardContent>
      </Card>
    </div>
  );
}
