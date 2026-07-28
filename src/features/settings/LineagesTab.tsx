import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Plus, Upload } from 'lucide-react';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
  DialogClose,
} from '@/components/ui/dialog';
import { LineageCard } from '@/features/lineageManager/LineageCard';
import {
  NewLineageDialog,
  type NewLineagePayload,
} from '@/features/lineageManager/NewLineageDialog';
import {
  ImportLineageDialog,
  type ImportLineagePayload,
} from '@/features/lineageManager/ImportLineageDialog';
import { pickLineageExportPath } from '@/ipc/lineages';
import { useLineagesStore } from '@/store/lineages';
import { useSignaturesStore } from '@/store/signatures';

export function LineagesTab() {
  const { t } = useTranslation();
  const { list, refresh, create, import: importLineage, remove, export: exportLineage } = useLineagesStore();
  const signatures = useSignaturesStore((s) => s.list);
  const refreshSignatures = useSignaturesStore((s) => s.refresh);
  const [newOpen, setNewOpen] = useState(false);
  const [importOpen, setImportOpen] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState<{ id: string; label: string } | null>(null);

  useEffect(() => {
    refresh().catch((error) => toast.error(String(error)));
    refreshSignatures().catch(() => undefined);
  }, [refresh, refreshSignatures]);

  const signatureOptions = signatures.map((s) => ({ id: s.id, label: s.label }));
  const labelFor = (id: string) =>
    signatures.find((s) => s.id === id)?.label ?? id;

  return (
    <div className="space-y-6">
      <div>
        <h2 className="text-xl font-semibold text-text-0">{t('lineages.title')}</h2>
        <p className="text-sm text-text-2">{t('lineages.subtitle')}</p>
      </div>
      <div className="flex gap-2">
        <Button onClick={() => setNewOpen(true)} disabled={signatures.length < 2}>
          <Plus className="h-4 w-4" /> {t('lineages.new')}
        </Button>
        <Button variant="outline" onClick={() => setImportOpen(true)} disabled={signatures.length < 2}>
          <Upload className="h-4 w-4" /> {t('lineages.import')}
        </Button>
      </div>
      {signatures.length < 2 && (
        <div
          role="alert"
          className="flex items-start gap-2 rounded-lg border border-slate-200 bg-white px-3 py-2 text-sm font-medium shadow-sm"
        >
          <span className="mt-0.5 text-base leading-none text-slate-500">!</span>
          <span className="leading-relaxed text-slate-900">{t('lineages.needTwoSignatures')}</span>
        </div>
      )}
      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
        {list.map((lineage) => (
          <LineageCard
            key={lineage.config.id}
            lineage={lineage}
            oldLabel={labelFor(lineage.config.oldSignatureId)}
            newLabel={labelFor(lineage.config.newSignatureId)}
            onDelete={() =>
              setConfirmDelete({ id: lineage.config.id, label: lineage.config.label })
            }
            onExport={async () => {
              if (!lineage.fileExists) {
                toast.error(t('lineages.exportFailed', { error: t('lineages.statusFileMissing') }));
                return;
              }
              try {
                const defaultName = `${lineage.config.label || 'lineage'}.lineage`;
                const dest = await pickLineageExportPath(defaultName);
                if (!dest) return;
                const written = await exportLineage(lineage.config.id, dest);
                toast.success(t('lineages.exported', { path: written }));
              } catch (error) {
                toast.error(t('lineages.exportFailed', { error: String(error) }));
              }
            }}
          />
        ))}
        {list.length === 0 && (
          <p className="text-sm text-text-2">{t('lineages.empty')}</p>
        )}
      </div>

      <NewLineageDialog
        open={newOpen}
        onOpenChange={setNewOpen}
        signatures={signatureOptions}
        onSubmit={async (payload: NewLineagePayload) => {
          await create(payload);
          toast.success(t('lineages.created'));
        }}
      />
      <ImportLineageDialog
        open={importOpen}
        onOpenChange={setImportOpen}
        signatures={signatureOptions}
        onSubmit={async (payload: ImportLineagePayload) => {
          await importLineage(payload);
          toast.success(t('lineages.imported'));
        }}
      />

      <Dialog
        open={confirmDelete !== null}
        onOpenChange={(open) => {
          if (!open) setConfirmDelete(null);
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t('lineages.deleteTitle')}</DialogTitle>
            <DialogDescription>
              {confirmDelete
                ? t('lineages.deleteDescription', { label: confirmDelete.label })
                : ''}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <DialogClose asChild>
              <Button variant="outline">{t('common.cancel')}</Button>
            </DialogClose>
            <Button
              variant="danger"
              onClick={async () => {
                if (!confirmDelete) return;
                try {
                  await remove(confirmDelete.id);
                  toast.success(t('lineages.deleted'));
                } catch (error) {
                  toast.error(String(error));
                } finally {
                  setConfirmDelete(null);
                }
              }}
            >
              {t('common.delete')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
