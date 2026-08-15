import { AdapterProfilesList, type AdapterProfilesListProps } from './AdapterProfilesList';
import {
  adapterErrorDetails,
  adapterErrorRetryHint,
  errorMessage,
} from './adapter-model';

export function AdapterErrorLines({
  error,
  fallback,
}: {
  error: unknown;
  fallback: string;
}) {
  const details = adapterErrorDetails(error);
  const retryHint = adapterErrorRetryHint(error);
  return (
    <>
      <p className="text-sm text-danger" role="alert">{errorMessage(error, fallback)}</p>
      {details ? <p className="text-xs text-secondary">{details}</p> : null}
      {retryHint ? <p className="text-xs text-secondary">{retryHint}</p> : null}
    </>
  );
}

/** Stable page-facing alias for the managed-profile service list. */
export function AdapterProfiles(props: AdapterProfilesListProps) {
  return <AdapterProfilesList {...props} />;
}
