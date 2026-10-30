import type {ReactNode} from 'react';
import clsx from 'clsx';
import styles from './styles.module.css';

function Node({name, role, code}: {name: string; role: string; code?: boolean}) {
  return (
    <div className={styles.node}>
      <span className={clsx(styles.name, code && styles.code)}>{name}</span>
      <span className={styles.role}>{role}</span>
    </div>
  );
}

function Arrow({label, sql}: {label: string; sql?: boolean}) {
  return (
    <div className={clsx(styles.arrow, sql && styles.sql)}>
      <span className={styles.arrowLabel}>{label}</span>
      <span className={styles.line} />
    </div>
  );
}

export default function WebAppDiagram(): ReactNode {
  return (
    <figure
      className={styles.diagram}
      role="img"
      aria-label="The browser calls a route of server.js. On your server, server.js sends SQL to µQuery, which reads the Parquet and CSV files.">
      <div className={styles.flow}>
        <Node name="Browser" role="Calls routes" />
        <Arrow label="GET /api/…" />
        <div className={styles.boundary}>
          <span className={styles.boundaryLabel}>Your server</span>
          <Node name="server.js" role="Holds the SQL" code />
          <Arrow label="SQL" sql />
          <Node name="µQuery" role="Runs the SQL" />
        </div>
        <Arrow label="HTTPS" />
        <Node name="Data files" role="Parquet and CSV" />
      </div>
    </figure>
  );
}
