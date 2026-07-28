import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import '@/i18n';
import { toast } from 'sonner';
import { Card, CardHeader, CardTitle, CardContent } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Switch } from '@/components/ui/switch';
import { pickApkFile, pickOutDir, decompileApk } from '@/ipc/decompile';
import type { TaskHandle } from '@/ipc/types';

type Props = { onStarted: (h: TaskHandle) => void };

export function DecompileForm({ onStarted }: Props) {
  const { t } = useTranslation();
  const [apk, setApk] = useState<string | null>(null);
  const [out, setOut] = useState<string | null>(null);
  const [force, setForce] = useState(false);
  const [busy, setBusy] = useState(false);

  async function pickApk() {
    try {
      const p = await pickApkFile();
      if (p) setApk(p);
    } catch (e) {
      toast.error(String(e));
    }
  }

  async function pickOut() {
    try {
      const d = await pickOutDir();
      if (d) setOut(d);
    } catch (e) {
      toast.error(String(e));
    }
  }

  async function start() {
    if (!apk || !out) return;
    setBusy(true);
    try {
      const handle = await decompileApk(apk, out, force);
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
      <CardHeader>
        <CardTitle>{t('nav.decompile')}</CardTitle>
      </CardHeader>
      <CardContent className="space-y-4">
        <Row label="APK">
          <div className="flex items-center gap-2">
            <Input value={apk ?? ''} readOnly placeholder="选 APK…" />
            <Button variant="outline" onClick={pickApk}>
              {t('common.open')}
            </Button>
          </div>
        </Row>
        <Row label="输出目录">
          <div className="flex items-center gap-2">
            <Input value={out ?? ''} readOnly placeholder="选输出目录…" />
            <Button variant="outline" onClick={pickOut}>
              {t('common.open')}
            </Button>
          </div>
        </Row>
        <div className="flex items-center gap-3">
          <Switch id="force" checked={force} onCheckedChange={setForce} />
          <label htmlFor="force" className="text-sm text-text-1">
            {t('decompile.force')}
          </label>
        </div>
        <Button onClick={start} disabled={!apk || !out || busy}>
          {busy ? t('decompile.starting') : t('decompile.start')}
        </Button>
      </CardContent>
    </Card>
  );
}

function Row({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="grid gap-1">
      <span className="text-xs text-text-2">{label}</span>
      {children}
    </div>
  );
}
