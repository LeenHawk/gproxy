export function getCredentialUsageActionLabels({
  expanded,
  loading,
  labels,
}: {
  expanded: boolean;
  loading: boolean;
  labels: {
    show: string;
    hide: string;
    refresh: string;
    loading: string;
  };
}) {
  if (loading) {
    return {
      primary: labels.loading,
      refresh: labels.loading,
    };
  }

  return {
    primary: expanded ? labels.hide : labels.show,
    refresh: labels.refresh,
  };
}
