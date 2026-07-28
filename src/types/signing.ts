export type SigningSchemes = {
  v1: boolean;
  v2: boolean;
  v3: boolean;
  v4: boolean;
};

export const DEFAULT_SIGNING_SCHEMES: SigningSchemes = {
  v1: true,
  v2: true,
  v3: true,
  v4: true,
};

export type SignatureInfo = {
  apkPath: string;
  fileSize: number;
  verifies: boolean;
  isSigned: boolean;
  verifiedV1: boolean;
  verifiedV2: boolean;
  verifiedV3: boolean;
  verifiedV31: boolean;
  verifiedV4: boolean;
  signerCount: number;
  errorMessage: string | null;
  rawOutput: string;
  /** Detailed per-signer certificate info. Newer backend fields; older
   *  APKs may emit an empty array. */
  signers?: SignerDetail[];
};

export type SignerDetail = {
  index: number;
  dn: string | null;
  issuerDn: string | null;
  sha256: string | null;
  sha1: string | null;
  md5: string | null;
  serial: string | null;
  validFrom: string | null;
  validTo: string | null;
  /** Public / private key algorithm label, e.g. "RSA", "EC", "DSA". */
  keyAlgorithm: string | null;
  /** Bit length of the public key. */
  keyBits: number | null;
  /** SHA-256 / SHA-1 / MD5 digests of the *public key* (not the certificate). */
  publicKeySha256: string | null;
  publicKeySha1: string | null;
  publicKeyMd5: string | null;
  /** Signature algorithm declared in the certificate (e.g. SHA256withRSA). */
  signatureAlgorithm: string | null;
  /** X.509 certificate version (typically 1 or 3). */
  certVersion: number | null;
  /** Heuristic — true when the DN / Issuer DN matches the standard Android
   *  debug keystore (CN=Android Debug + O=Android). */
  isDebugSigned: boolean;
  /** Bucketed key strength surfaced as a coloured badge on the UI. */
  keyStrength: 'weak' | 'acceptable' | 'strong' | 'unknown';
  schemes: string[];
  /** apksigner WARNING rows attached to this signer block (e.g. v1 scheme
   *  reporting an unprotected META-INF entry). */
  warnings: string[];
};

export type StripResult = {
  outputPath: string;
  outputSize: number;
  removedV1Files: string[];
  removedV2V3: boolean;
  removedV4Idsig: boolean;
  hadV1: boolean;
  hadV2V3: boolean;
  hadV4: boolean;
  sourcePath: string;
};


export type PackerIndicator = {
  kind: 'native' | 'entry_class' | string;
  value: string;
  packer: string;
};

export type PackerReport = {
  isPacked: boolean;
  packerName: string | null;
  indicators: PackerIndicator[];
};
