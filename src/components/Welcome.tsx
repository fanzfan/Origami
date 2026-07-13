import { useTranslation } from "react-i18next";
import { UiIcon } from "../icons";

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
  const { t } = useTranslation();
  return (
    <div className="welcome">
      <section className="welcome-card">
        <div className="welcome-brand">
          <span className="brand-logo welcome-logo" aria-hidden="true" />
          <div className="welcome-brand-copy">
            <h1>Origami</h1>
          </div>
        </div>

        <div className={`dropzone ${p.dragOver ? "over" : ""}`}>
          <UiIcon className="drop-icon" name="archive" size={28} />
          <p>{p.loading ? t("welcome.reading") : t("welcome.dropHint")}</p>
          <div className="actions">
            <button className="btn primary" onClick={p.onOpen} disabled={p.loading}>
              <UiIcon name="archive" />
              {t("welcome.openArchive")}
            </button>
            <button className="btn secondary" onClick={p.onBrowse} disabled={p.loading}>
              <UiIcon name="folder-open" />
              {t("welcome.browseFiles")}
            </button>
            <button className="btn secondary" onClick={p.onCompressFiles} disabled={p.loading}>
              <UiIcon name="file-archive" />
              {t("welcome.compressFiles")}
            </button>
            <button className="btn secondary" onClick={p.onCompressFolder} disabled={p.loading}>
              <UiIcon name="folder-archive" />
              {t("welcome.compressFolder")}
            </button>
          </div>
        </div>
      </section>

      {p.recent.length > 0 && (
        <div className="recent">
          <h3>{t("welcome.recent")}</h3>
          {p.recent.map((path) => (
            <button type="button" key={path} className="item" onClick={() => p.onOpenRecent(path)}>
              <UiIcon name="archive" size={17} />
              <span className="name">{path.split(/[\\/]/).pop()}</span>
              <span className="path">{path}</span>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
