import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { CheckCircle2, Copy, Crown, Loader2, Trash2 } from 'lucide-react';
import { toast } from 'sonner';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Textarea } from '@/components/ui/textarea';
import { useLicenseStore } from '@/store/license';

export function LicenseTab() {
  const { t, i18n } = useTranslation();
  const { status, loading, error, activate, remove } = useLicenseStore();
  const [token, setToken] = useState('');
  const active = status?.state === 'active';

  async function submit() {
    if (!token.trim()) return;
    try { await activate(token); setToken(''); toast.success(t('license.activated')); }
    catch (e) { toast.error(String(e)); }
  }

  async function copyDevice() {
    if (!status?.deviceId) return;
    await navigator.clipboard.writeText(status.deviceId);
    toast.success(t('license.deviceCopied'));
  }

  return (
    <div className="space-y-4">
      <Card>
        <CardHeader><CardTitle className="flex items-center gap-2"><Crown className="h-5 w-5 text-warning" />{t('license.statusTitle')}</CardTitle></CardHeader>
        <CardContent className="space-y-3 text-sm">
          <div className="flex items-center gap-2">
            <Badge variant={active ? 'success' : status?.state === 'expired' ? 'danger' : 'secondary'}>
              {active ? t('license.vip') : t(`license.state.${status?.state ?? 'unlicensed'}`)}
            </Badge>
            {status?.licensedTo && <span>{status.licensedTo}</span>}
          </div>
          {active && <div>{status.perpetual ? t('license.perpetual') : t('license.expiresAt', { date: status.expiresAt ? new Date(status.expiresAt).toLocaleDateString(i18n.language) : '-' })}</div>}
          {active && status.features.map((feature) => <div key={feature} className="flex gap-2"><CheckCircle2 className="h-4 w-4 text-success" />{t(`license.feature.${feature}`)}</div>)}
          {!active && (
            <div className="space-y-3 pt-1">
              <p className="text-xs font-medium text-text-2">{t('license.vipBenefits')}</p>
              <div className="grid gap-2 sm:grid-cols-2">
                <div className="flex items-center gap-2 text-text-1"><CheckCircle2 className="h-4 w-4 text-brand" />{t('license.feature.apk_report_export')}</div>
                <div className="flex items-center gap-2 text-text-1"><CheckCircle2 className="h-4 w-4 text-brand" />{t('license.feature.signing_v31')}</div>
                <div className="flex items-center gap-2 text-text-1"><CheckCircle2 className="h-4 w-4 text-brand" />{t('license.feature.adb_multi_device')}</div>
                <div className="flex items-center gap-2 text-text-1"><CheckCircle2 className="h-4 w-4 text-brand" />{t('license.feature.adb_batch_install')}</div>
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
          {active && <Button variant="outline" onClick={() => void remove()} disabled={loading}><Trash2 className="h-4 w-4" />{t('license.remove')}</Button>}
        </CardContent>
      </Card>
      <Card>
        <CardHeader><CardTitle>{t('license.deviceTitle')}</CardTitle></CardHeader>
        <CardContent className="space-y-3">
          <p className="text-sm text-text-2">{t('license.deviceHint')}</p>
          <div className="flex gap-2"><code className="flex-1 rounded-md bg-bg-2 px-3 py-2 text-sm">{status?.deviceId ?? '-'}</code><Button variant="outline" onClick={() => void copyDevice()}><Copy className="h-4 w-4" />{t('license.copy')}</Button></div>
        </CardContent>
      </Card>
      {!active && <Card>
        <CardHeader><CardTitle>{t('license.activateTitle')}</CardTitle></CardHeader>
        <CardContent className="space-y-3">
          <p className="text-sm text-text-2">{t('license.activateHint')}</p>
          <Textarea value={token} onChange={(e) => setToken(e.target.value)} placeholder="JADB1..." className="min-h-28 font-mono" />
          <Button onClick={() => void submit()} disabled={loading || !token.trim()}>{loading && <Loader2 className="h-4 w-4 animate-spin" />}{t('license.activate')}</Button>
        </CardContent>
      </Card>}
    </div>
  );
}
