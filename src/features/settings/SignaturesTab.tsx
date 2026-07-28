import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Plus, Upload } from 'lucide-react';
import { toast } from 'sonner';
import { Tabs, TabsList, TabsTrigger, TabsContent } from '@/components/ui/tabs';
import { Button } from '@/components/ui/button';
import { SignatureCard } from '@/features/signatureManager/SignatureCard';
import {
  NewSignatureDialog,
  type NewSignaturePayload,
} from '@/features/signatureManager/NewSignatureDialog';
import {
  ImportKeystoreDialog,
  type ImportKeystorePayload,
} from '@/features/signatureManager/ImportKeystoreDialog';
import { useSignaturesStore } from '@/store/signatures';
import { LineagesTab } from '@/features/settings/LineagesTab';

export function SignaturesTab() {
  const { t } = useTranslation();
  const { list, refresh, create, remove, import: importSignature, exportKeystore } = useSignaturesStore();
  const [sub, setSub] = useState<'configs' | 'lineages'>('configs');
  const [newOpen, setNewOpen] = useState(false);
  const [importOpen, setImportOpen] = useState(false);

  useEffect(() => {
    refresh().catch((error) => toast.error(String(error)));
  }, [refresh]);

  return (
    <div className="space-y-6">
      <div>
        <h2 className="text-xl font-semibold text-text-0">{t('signatures.title')}</h2>
        <p className="text-sm text-text-2">{t('signatures.subtitle')}</p>
      </div>
      <Tabs value={sub} onValueChange={(v) => {
        if (v === 'configs' || v === 'lineages') setSub(v);
      }}>
        <TabsList>
          <TabsTrigger value="configs">{t('signatures.tabConfigs')}</TabsTrigger>
          <TabsTrigger value="lineages">{t('signatures.tabLineages')}</TabsTrigger>
        </TabsList>
        <TabsContent value="configs" className="grid gap-4">
          <div className="flex gap-2">
            <Button onClick={() => setNewOpen(true)}>
              <Plus className="h-4 w-4" /> {t('signatures.new')}
            </Button>
            <Button variant="outline" onClick={() => setImportOpen(true)}>
              <Upload className="h-4 w-4" /> {t('signatures.import')}
            </Button>
          </div>
          <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
            {list.map((signature) => (
              <SignatureCard
                key={signature.id}
                sig={signature}
                onExport={() =>
                  exportKeystore(signature.id, `${signature.label || 'signature'}.jks`)
                }
                onDelete={async () => {
                  try {
                    await remove(signature.id);
                    toast.success(t('signatures.deleted'));
                  } catch (error) {
                    toast.error(String(error));
                  }
                }}
              />
            ))}
            {list.length === 0 && (
              <p className="text-sm text-text-2">{t('signatures.empty')}</p>
            )}
          </div>
        </TabsContent>
        <TabsContent value="lineages">
          <LineagesTab />
        </TabsContent>
      </Tabs>

      <NewSignatureDialog
        open={newOpen}
        onOpenChange={setNewOpen}
        onSubmit={async (payload: NewSignaturePayload) => {
          await create(payload);
          toast.success(t('signatures.created'));
        }}
      />
      <ImportKeystoreDialog
        open={importOpen}
        onOpenChange={setImportOpen}
        onSubmit={async (payload: ImportKeystorePayload) => {
          await importSignature(
            payload.srcPath,
            payload.alias,
            payload.password,
            payload.label,
          );
          toast.success(t('signatures.imported'));
        }}
      />
    </div>
  );
}
