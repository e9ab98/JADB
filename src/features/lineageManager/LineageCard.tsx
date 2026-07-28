import { useTranslation } from 'react-i18next';
import {
  Card,
  CardHeader,
  CardTitle,
  CardDescription,
  CardContent,
} from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Download, Trash2 } from 'lucide-react';
import type { LineageStatus } from '@/types/lineage';

type Props = {
  lineage: LineageStatus;
  oldLabel: string;
  newLabel: string;
  onDelete: () => void;
  onExport: () => void;
};

export function LineageCard({ lineage, oldLabel, newLabel, onDelete, onExport }: Props) {
  const { t } = useTranslation();
  const statusKey = !lineage.fileExists
    ? 'lineages.statusFileMissing'
    : !lineage.oldSignatureExists || !lineage.newSignatureExists
    ? 'lineages.statusSignatureMissing'
    : 'lineages.statusReady';
  return (
    <Card>
      <CardHeader>
        <CardTitle>{lineage.config.label}</CardTitle>
        <CardDescription className="font-mono text-xs break-all">
          {oldLabel} → {newLabel}
        </CardDescription>
      </CardHeader>
      <CardContent className="flex items-center justify-between gap-2">
        <span className="text-xs text-text-2">
          {t(statusKey)} · {new Date(lineage.config.createdAt).toLocaleString()}
        </span>
        <div className="flex gap-1">
          <Button
            variant="outline"
            size="icon"
            onClick={onExport}
            disabled={!lineage.fileExists}
            aria-label={t('lineages.export')}
            title={lineage.fileExists ? t('lineages.export') : t('lineages.statusFileMissing')}
          >
            <Download className="h-4 w-4" />
          </Button>
          <Button variant="outline" size="icon" onClick={onDelete} aria-label={t('common.delete')}>
            <Trash2 className="h-4 w-4" />
          </Button>
        </div>
      </CardContent>
    </Card>
  );
}
