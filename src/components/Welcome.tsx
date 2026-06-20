interface Props {
  dragOver: boolean;
  loading: boolean;
  recent: string[];
  onOpen: () => void;
  onOpenRecent: (path: string) => void;
  onCompressFiles: () => void;
  onCompressFolder: () => void;
  onBrowse: () => void;
}

export function Welcome(p: Props) {
  return (
    <div className="welcome">
      <div className={`dropzone ${p.dragOver ? "over" : ""}`}>
        <div className="logo">📦</div>
        <h1>Origami</h1>
        <p>{p.loading ? "正在读取归档…" : "拖入压缩文件以打开，或拖入普通文件以压缩"}</p>
        <div className="actions">
          <button className="btn primary" onClick={p.onOpen} disabled={p.loading}>
            打开归档…
          </button>
          <button className="btn" onClick={p.onBrowse} disabled={p.loading}>
            浏览文件…
          </button>
          <button className="btn" onClick={p.onCompressFiles} disabled={p.loading}>
            压缩文件…
          </button>
          <button className="btn" onClick={p.onCompressFolder} disabled={p.loading}>
            压缩文件夹…
          </button>
        </div>
      </div>

      {p.recent.length > 0 && (
        <div className="recent">
          <h3>最近打开</h3>
          {p.recent.map((path) => (
            <div key={path} className="item" onClick={() => p.onOpenRecent(path)}>
              <span>🗜️</span>
              <span className="name">{path.split("/").pop()}</span>
              <span className="path">{path}</span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
