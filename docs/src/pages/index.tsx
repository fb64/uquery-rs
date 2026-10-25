import clsx from 'clsx';
import Link from '@docusaurus/Link';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';
import useBaseUrl from '@docusaurus/useBaseUrl';
import Layout from '@theme/Layout';
import HomepageFeatures from '@site/src/components/HomepageFeatures';
import Heading from '@theme/Heading';
import {JSX, useState} from 'react';

import styles from './index.module.css';

const INSTALL_CMD = 'curl -fsSL https://install.uquery.dev | sh';
// Hosted on Vercel Blob to keep the video out of the git history.
const INTRO_VIDEO_URL = 'https://6s7zwsj7te57nmvt.public.blob.vercel-storage.com/uquery-intro.mp4';

function InstallCommand() {
  const [copied, setCopied] = useState(false);

  const copy = () => {
    navigator.clipboard.writeText(INSTALL_CMD).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    }).catch(() => setCopied(false));
  };

  return (
    <div className={styles.installPanel}>
      <code className={styles.installCmd}>{INSTALL_CMD}</code>
      <button className={styles.copyBtn} onClick={copy} title="Copy to clipboard">
        {copied ? '✓' : (
          <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <rect x="9" y="9" width="13" height="13" rx="2"/>
            <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/>
          </svg>
        )}
      </button>
    </div>
  );
}

function HomepageHeader() {
  const {siteConfig} = useDocusaurusContext();
  return (
    <header className={clsx('hero hero--primary', styles.heroBanner)}>
      <div className="container">
        <Heading as="h1" className="hero__title">
          <span className={styles.mu}>µ</span>Query
        </Heading>
        <p className="hero__subtitle">{siteConfig.tagline}</p>
        <InstallCommand />
        <div className={styles.buttons}>
          <Link
            className="button button--secondary button--lg"
            to="/docs/quick-start">
            µQuery QuickStart ⏱️
          </Link>
        </div>
      </div>
    </header>
  );
}

function IntroVideo() {
  return (
    <section className={styles.videoSection}>
      <div className="container">
        <Heading as="h2" className={styles.videoTitle}>µQuery in 90 seconds</Heading>
        <video
          className={styles.video}
          controls
          playsInline
          preload="metadata"
          poster={useBaseUrl('/video/uquery-intro-poster.jpg')}>
          <source src={INTRO_VIDEO_URL} type="video/mp4" />
        </video>
      </div>
    </section>
  );
}

export default function Home(): JSX.Element {
  const {siteConfig} = useDocusaurusContext();
  return (
    <Layout
      title="SQL over HTTP for CSV, Parquet & JSON files"
      description="uQuery is a lightweight HTTP API to query CSV, JSON, Parquet and Excel files with SQL. Powered by DuckDB, built in Rust.">
      <HomepageHeader />
      <main>
        <IntroVideo />
        <HomepageFeatures />
      </main>
    </Layout>
  );
}
