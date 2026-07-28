import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import { Download, Trash2 } from 'lucide-react';
import {
  Card,
  CardHeader,
  CardTitle,
  CardDescription,
  CardContent,
} from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import type { SignatureConfig } from '@/ipc/signatures';

type Props = {
  sig: SignatureConfig;
  onDelete: () => void;
  onExport: () => Promise<string | null>;
};

export function SignatureCard({ sig, onDelete, onExport }: Props) {
  const { t } = useTranslation();
  const [exporting, setExporting] = useState(false);

  async function handleExport() {
    if (exporting) return;
    setExporting(true);
    try {
      const written = await onExport();
      if (written) {
        toast.success(t('signatures.exportSuccess', { path: written }));
      }
    } catch (error) {
      toast.error(String(error));
    } finally {
      setExporting(false);
    }
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>{sig.label}</CardTitle>
        <CardDescription className="font-mono text-xs break-all">
          {sig.keystorePath}
        </CardDescription>
      </CardHeader>
      <CardContent className="flex items-center justify-between gap-2">
        <span className="text-xs text-text-2">
          alias: <span className="font-mono">{sig.keyAlias}</span>
        </span>
        <div className="flex gap-1">
          <Button
            variant="outline"
            size="icon"
            onClick={() => void handleExport()}
            disabled={exporting}
            aria-label={t('signatures.export')}
            title={t('signatures.export')}
          >
            <Download className="h-4 w-4" />
          </Button>
          <Button
            variant="outline"
            size="icon"
            onClick={onDelete}
            aria-label={t('common.delete')}
          >
            <Trash2 className="h-4 w-4" />
          </Button>
        </div>
      </CardContent>
    </Card>
  );
}
