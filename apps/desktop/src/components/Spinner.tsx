export function Spinner({ label }: { label?: string }) {
  return (
    <span className="ui-spinner" role="status" aria-label={label ?? "加载中"}>
      <span className="ui-spinner__ring" aria-hidden="true" />
      {label === undefined ? null : <span>{label}</span>}
    </span>
  );
}
