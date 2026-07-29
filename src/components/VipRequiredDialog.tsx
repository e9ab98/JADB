import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import { Copy } from 'lucide-react';
import { toast } from 'sonner';
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { useLicenseStore } from '@/store/license';
import { openLicenseCenter } from '@/ipc/window';

export function VipRequiredDialog() {
  const { t } = useTranslation();
  const feature = useLicenseStore((s) => s.promptFeature);
  const close = useLicenseStore((s) => s.closePrompt);
  return (
    <Dialog open={feature !== null} onOpenChange={(open) => !open && close()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{t('license.vipRequiredTitle')}</DialogTitle>
          <DialogDescription className="space-y-2">
            <span className="block">{t(`license.feature.${feature ?? 'apk_report_export'}`)}</span>
            <span className="block font-medium text-text-0">{t('license.purchaseHint')}</span>
          </DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button variant="outline" onClick={() => void navigator.clipboard.writeText('godfeer').then(() => toast.success(t('license.wechatCopied')))}><Copy className="h-4 w-4" />{t('license.copyWechat')}</Button>
          <Button onClick={() => { close(); void openLicenseCenter().catch((error) => toast.error(String(error))); }}>{t('license.goAuthorize')}</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
