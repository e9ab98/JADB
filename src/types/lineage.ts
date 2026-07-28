export type LineageConfig = {
  id: string;
  label: string;
  lineagePath: string;
  oldSignatureId: string;
  newSignatureId: string;
  createdAt: string;
};

export type LineageStatus = {
  config: LineageConfig;
  fileExists: boolean;
  oldSignatureExists: boolean;
  newSignatureExists: boolean;
};

export const ROTATION_MIN_SDK_VERSION = 33;
