import { HashRouter, Navigate, Routes, Route, useLocation } from 'react-router-dom';
import { Toaster } from 'sonner';
import { Sidebar } from '@/components/Sidebar';
import { SettingsView } from '@/views/SettingsView';
import { AnalyzeView } from '@/views/AnalyzeView';
import { AppsToolsView } from '@/views/AppsToolsView';
import { DecompileView } from '@/views/DecompileView';
import { RepackageView } from '@/views/RepackageView';
import { SignView } from '@/views/SignView';
import { AdbView } from '@/views/AdbView';
import { AppsView } from '@/views/AppsView';
import { DataDirView } from '@/views/DataDirView';
import { useUpdateCheck } from '@/hooks/useUpdateCheck';
import { useEffect } from 'react';
import { useLicenseStore } from '@/store/license';
import { VipRequiredDialog } from '@/components/VipRequiredDialog';

export default function App() {
  useUpdateCheck();
  return (
    <HashRouter>
      <Shell />
    </HashRouter>
  );
}

/**
 * Routes that render without the sidebar. These are the standalone
 * OS-level windows opened via the `open_*_window` Rust commands — each
 * gets its own window so the user can keep working in the main shell.
 */
const STANDALONE_ROUTES = new Set<string>(['/apps', '/data-dir', '/decompile', '/repackage', '/analyze']);

/**
 * Outer chrome. The apps / data-dir / decompile / repackage views are
 * rendered inside their own OS-level windows (see `open_*_window` Rust
 * commands) and do not need the sidebar.
 */
function Shell() {
  const refreshLicense = useLicenseStore((s) => s.refresh);
  useEffect(() => { void refreshLicense(); }, [refreshLicense]);
  const location = useLocation();
  const isStandaloneWindow = STANDALONE_ROUTES.has(location.pathname);
  return (
    <div className="flex h-screen w-screen bg-bg-0 text-text-0">
      {!isStandaloneWindow && <Sidebar />}
      <main className="flex-1 overflow-auto">
        <Routes>
          <Route path="/" element={<Navigate to="/adb" replace />} />
          <Route path="/analyze" element={<AnalyzeView />} />
          <Route path="/decompile" element={<DecompileView />} />
          <Route path="/repackage" element={<RepackageView />} />
          <Route path="/sign" element={<SignView />} />
          <Route
            path="/signatures"
            element={<Navigate to="/settings?tab=signatures" replace />}
          />
          <Route path="/adb" element={<AdbView />} />
          <Route path="/apps-tools" element={<AppsToolsView />} />
          <Route path="/apps" element={<AppsView />} />
          <Route path="/data-dir" element={<DataDirView />} />
          <Route path="/settings" element={<SettingsView />} />
        </Routes>
        <VipRequiredDialog />
        <Toaster richColors position="top-right" />
      </main>
    </div>
  );
}
