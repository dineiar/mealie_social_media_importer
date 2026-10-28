export function Brand() {
  return (
    <div className="brand">
      <svg className="brand__mark" viewBox="0 0 32 32" aria-hidden="true">
        <rect width="32" height="32" rx="9" fill="currentColor" />
        <path
          d="M16 7.5v10.5m-4.2-4.2 4.2 4.2 4.2-4.2M9 20.5c0 2.9 3.1 5 7 5s7-2.1 7-5"
          fill="none"
          stroke="#FFFFFF"
          strokeWidth="2.2"
          strokeLinecap="round"
          strokeLinejoin="round"
        />
      </svg>
      <span className="brand__name">Mealie Social Media Importer</span>
    </div>
  );
}
