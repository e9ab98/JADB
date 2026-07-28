import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import '@/i18n';
import { toast } from 'sonner';
import { Card, CardContent } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Switch } from '@/components/ui/switch';
import { pickSrcDir, pickOutApk, repackageApk } from '@/ipc/repackage';
import type { TaskHandle } from '@/ipc/types';
import { listSignatures, type SignatureConfig } from '@/ipc/signatures';
import { SigningSchemeSelector } from '@/features/sign/SigningSchemeSelector';
import {
  DEFAULT_SIGNING_SCHEMES,
  type SigningSchemes,
} from '@/types/signing';

type Props = { onStarted: (h: TaskHandle) => void };

export function RepackageForm({ onStarted }: Props) {
  const { t } = useTranslation();
  const [srcDir, setSrcDir] = useState<string | null>(null);
  const [outApk, setOutApk] = useState<string | null>(null);
  const [sign, setSign] = useState(false);
  const [sigId, setSigId] = useState<string | null>(null);
  const [sigs, setSigs] = useState<SignatureConfig[]>([]);
  const [busy, setBusy] = useState(false);
  const [schemes, setSchemes] = useState<SigningSchemes>({ ...DEFAULT_SIGNING_SCHEMES });

  useEffect(() => {
    void listSignatures()
      .then(setSigs)
      .catch((e) => toast.error(String(e)));
  }, []);

  async function start() {
    if (!srcDir || !outApk) return;
    if (sign && !sigId) return;
    setBusy(true);
    try {
      const handle = await repackageApk(srcDir, outApk, sign, sigId, schemes);
      onStarted(handle);
      toast.success(handle.task_id);
    } catch (e) {
      toast.error(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <Card>
      <CardContent className="grid gap-3">
        <div className="flex gap-2">
          <Input readOnly value={srcDir ?? ''} placeholder="反编译目录（含 apktool.yml）" />
          <Button variant="outline" onClick={async () => setSrcDir(await pickSrcDir())}>
            {t('common.open')}
          </Button>
        </div>
        <div className="flex gap-2">
          <Input readOnly value={outApk ?? ''} placeholder="输出 APK" />
          <Button variant="outline" onClick={async () => setOutApk(await pickOutApk())}>
            {t('common.save')}
          </Button>
        </div>
        <div className="flex items-center gap-2 text-sm text-text-1">
          <Switch id="sign" checked={sign} onCheckedChange={setSign} />
          <label htmlFor="sign">{t('repackage.signAfter')}</label>
        </div>
        {sign && (
          <div className="grid gap-3">
            <select
              value={sigId ?? ''}
              onChange={(e) => setSigId(e.target.value || null)}
              disabled={busy}
              className="h-9 rounded-md border border-border bg-bg-1 px-2 text-sm text-text-0 disabled:opacity-50"
            >
              <option value="">选择签名</option>
              {sigs.map((s) => (
                <option key={s.id} value={s.id}>
                  {s.label}
                </option>
              ))}
            </select>
            <SigningSchemeSelector value={schemes} onChange={setSchemes} disabled={busy} />
          </div>
        )}
        <Button
          disabled={busy || !srcDir || !outApk || (sign && !sigId)}
          onClick={start}
        >
          {busy ? t('repackage.starting') : t('repackage.start')}
        </Button>
      </CardContent>
    </Card>
  );
}
