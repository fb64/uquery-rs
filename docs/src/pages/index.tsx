import clsx from 'clsx';
import Head from '@docusaurus/Head';
import Link from '@docusaurus/Link';
import useDocusaurusContext from '@docusaurus/useDocusaurusContext';
import useBaseUrl from '@docusaurus/useBaseUrl';
import CodeBlock from '@theme/CodeBlock';
import Layout from '@theme/Layout';
import HomepageFeatures from '@site/src/components/HomepageFeatures';
import Heading from '@theme/Heading';
import {JSX, useState} from 'react';

import styles from './index.module.css';

const INSTALL_CMD = 'curl -fsSL https://install.uquery.dev | sh';
// Hosted on Vercel Blob to keep the video out of the git history.
const INTRO_VIDEO_URL = 'https://6s7zwsj7te57nmvt.public.blob.vercel-storage.com/uquery-intro.mp4';
const GITHUB_URL = 'https://github.com/fb64/uquery-rs';
const DESCRIPTION =
  'µQuery (uQuery) is a lightweight HTTP API to query CSV, JSON, Parquet and Excel files with SQL, powered by DuckDB and built in Rust.';

const EXAMPLE_QUERY = `curl -X POST http://localhost:8080 \\
  -H "Accept: application/json" \\
  -H "Content-Type: text/plain" \\
  -d "select * from 'https://raw.githubusercontent.com/duckdb/duckdb-web/main/data/weather.csv'"`;

// Captured from the fb64/uquery:0.8 image, one row per line.
const EXAMPLE_RESPONSE = `[
  {"column0":"San Francisco","column1":46,"column2":50,"column3":0.25,"column4":"1994-11-27"},
  {"column0":"San Francisco","column1":43,"column2":57,"column3":0.0,"column4":"1994-11-29"},
  {"column0":"Hayward","column1":37,"column2":54,"column4":"1994-11-29"}
]`;

type DocLink = {label: string; to: string};

type UseCase = {title: string; text: string; links: DocLink[]};

const USE_CASES: UseCase[] = [
  {
    title: 'Query remote files',
    text: 'Read files over HTTPS, in Amazon S3 or in Google Cloud Storage without downloading them first. The AWS and GCP credential chains handle authentication.',
    links: [
      {label: 'Query files', to: '/docs/advanced-tutorials/query-files'},
      {label: 'Cloud storage', to: '/docs/configuration#cloud-storage'},
    ],
  },
  {
    title: 'Query Iceberg tables',
    text: 'Attach an Iceberg REST catalog at startup, then query its tables directly by name.',
    links: [{label: 'Iceberg configuration', to: '/docs/configuration#iceberg'}],
  },
  {
    title: 'Deploy serverless',
    text: 'Run µQuery on AWS Lambda or Google Cloud Run. The Rust binary comes with pre-loaded DuckDB extensions, so cold starts take milliseconds.',
    links: [
      {label: 'AWS Lambda', to: '/docs/advanced-tutorials/cloud-providers/aws-serverless'},
      {label: 'Google Cloud Run', to: '/docs/advanced-tutorials/cloud-providers/gcp-serverless'},
    ],
  },
  {
    title: 'Pick a response format',
    text: 'Get JSON, JSON Lines, CSV or Apache Arrow IPC by setting the Accept header. Every format streams as soon as the first batch is ready.',
    links: [{label: 'Response formats', to: '/docs/response-formats'}],
  },
];

// Shown on the page and in the FAQPage JSON-LD. Text between backticks is rendered as code.
type Faq = {question: string; answer: string; link: DocLink};

const FAQS: Faq[] = [
  {
    question: 'How do I install µQuery?',
    answer:
      'Run `curl -fsSL https://install.uquery.dev | sh`, use the `fb64/uquery` Docker image, download a prebuilt binary for Linux, macOS or Windows from GitHub Releases, or build it with `cargo install`.',
    link: {label: 'Quick Start', to: '/docs/quick-start'},
  },
  {
    question: 'Which file formats can µQuery query?',
    answer:
      'CSV, JSON, Parquet, Excel and the other file formats DuckDB supports. Files can be local, on HTTPS, in Amazon S3 or in Google Cloud Storage, and you can also query the tables of an Iceberg catalog.',
    link: {label: 'Query files', to: '/docs/advanced-tutorials/query-files'},
  },
  {
    question: 'Which response formats does µQuery support?',
    answer:
      'JSON (the default), JSON Lines, CSV and Apache Arrow IPC. Choose one with the `Accept` header. All formats are streamed, so large results start arriving right away.',
    link: {label: 'Response formats', to: '/docs/response-formats'},
  },
  {
    question: 'How do I protect µQuery with an API key?',
    answer:
      'Start µQuery with `--api-key <key>` or set `UQ_API_KEY`. Clients then send the key as `Authorization: Bearer <key>` or `X-API-Key: <key>`, and requests without it get HTTP 401. Pass `--api-key` without a value to generate a random key at startup.',
    link: {label: 'API key', to: '/docs/configuration#api-key'},
  },
  {
    question: 'What can clients do with the default settings?',
    answer:
      'Only run single `SELECT` queries: other statements get HTTP 403 unless you add `--allow-writes`. Queries can read files in the current directory and remote `https://`, S3 and Google Cloud Storage URLs, which you can narrow with `--allowed-directories` and `--allowed-urls`. The binary only accepts local connections and rejects browser requests unless CORS is enabled. The query API is open until you set an API key.',
    link: {label: 'Read-only queries', to: '/docs/configuration#read-only-queries'},
  },
];

function plainText(text: string): string {
  return text.replaceAll('`', '');
}

function RichText({text}: {text: string}) {
  return (
    <>
      {text.split('`').map((part, idx) => (idx % 2 === 1 ? <code key={idx}>{part}</code> : part))}
    </>
  );
}

function StructuredData() {
  const {siteConfig} = useDocusaurusContext();
  const url = `${siteConfig.url}${siteConfig.baseUrl}`;
  const data = [
    {
      '@context': 'https://schema.org',
      '@type': 'SoftwareApplication',
      name: 'µQuery',
      alternateName: 'uQuery',
      description: DESCRIPTION,
      applicationCategory: 'DeveloperApplication',
      operatingSystem: 'Linux, macOS, Windows',
      offers: {'@type': 'Offer', price: '0', priceCurrency: 'USD'},
      license: 'https://www.apache.org/licenses/LICENSE-2.0',
      url,
      sameAs: [GITHUB_URL],
    },
    {
      '@context': 'https://schema.org',
      '@type': 'WebSite',
      name: 'µQuery',
      url,
    },
    {
      '@context': 'https://schema.org',
      '@type': 'FAQPage',
      mainEntity: FAQS.map(({question, answer, link}) => ({
        '@type': 'Question',
        name: question,
        // Same text as the visible answer, link label included.
        acceptedAnswer: {'@type': 'Answer', text: `${plainText(answer)} See ${link.label}.`},
      })),
    },
  ];
  return (
    <Head>
      {data.map((block, idx) => (
        <script key={idx} type="application/ld+json">{JSON.stringify(block)}</script>
      ))}
    </Head>
  );
}

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
        <p className={styles.brand}>
          <span className={styles.mu}>µ</span>Query
        </p>
        <Heading as="h1" className={clsx('hero__title', styles.heroTitle)}>
          Query CSV, Parquet and JSON files with SQL over HTTP
        </Heading>
        <p className="hero__subtitle">{siteConfig.tagline}</p>
        <InstallCommand />
        <div className={styles.buttons}>
          <Link
            className="button button--secondary button--lg"
            to="/docs/quick-start">
            µQuery Quick Start ⏱️
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
        <p className={styles.videoSummary}>
          Install µQuery with one line and start it with one command. Then POST a SQL query that reads
          a file on your machine, over HTTPS, in Amazon S3, in Google Cloud Storage or in an Iceberg
          catalog, and pick JSON, JSON Lines, CSV or Apache Arrow with the <code>Accept</code> header.
          Ship it as a single binary, a Docker image, on Kubernetes, AWS Lambda or Google Cloud Run.
        </p>
      </div>
    </section>
  );
}

function QueryExample() {
  return (
    <section className={styles.section}>
      <div className={clsx('container', styles.narrow)}>
        <Heading as="h2" className="text--center">Run your first query</Heading>
        <p>
          µQuery (uQuery) exposes a single <code>POST /</code> endpoint: send a SQL query as the request
          body and get the results back. This query reads a CSV file over HTTPS:
        </p>
        <CodeBlock language="bash">{EXAMPLE_QUERY}</CodeBlock>
        <p>
          The response is a JSON array with one object per row. The file has no header row, so DuckDB
          names the columns <code>column0</code> to <code>column4</code>:
        </p>
        <CodeBlock language="json">{EXAMPLE_RESPONSE}</CodeBlock>
        <p>
          Install µQuery and run this example in the <Link to="/docs/quick-start">Quick Start</Link>.
        </p>
      </div>
    </section>
  );
}

function UseCases() {
  return (
    <section className={styles.section}>
      <div className="container">
        <Heading as="h2" className="text--center">Use cases</Heading>
        <div className="row">
          {USE_CASES.map(({title, text, links}) => (
            <div key={title} className={clsx('col col--3', styles.useCase)}>
              <div className="card shadow--md">
                <div className="card__body">
                  <Heading as="h3">{title}</Heading>
                  <p>{text}</p>
                </div>
                <div className="card__footer">
                  {links.map(({label, to}, idx) => (
                    <span key={to}>
                      {idx > 0 && ' · '}
                      <Link to={to}>{label}</Link>
                    </span>
                  ))}
                </div>
              </div>
            </div>
          ))}
        </div>
      </div>
    </section>
  );
}

function FaqSection() {
  return (
    <section className={styles.section}>
      <div className={clsx('container', styles.narrow)}>
        <Heading as="h2" className="text--center">Frequently asked questions</Heading>
        {FAQS.map(({question, answer, link}) => (
          <div key={question} className={styles.faqItem}>
            <Heading as="h3">{question}</Heading>
            <p>
              <RichText text={answer} /> See <Link to={link.to}>{link.label}</Link>.
            </p>
          </div>
        ))}
      </div>
    </section>
  );
}

export default function Home(): JSX.Element {
  return (
    <Layout
      title="SQL over HTTP for CSV, Parquet & JSON files"
      description="uQuery is a lightweight HTTP API to query CSV, JSON, Parquet and Excel files with SQL. Powered by DuckDB, built in Rust.">
      <StructuredData />
      <HomepageHeader />
      <main>
        <IntroVideo />
        <HomepageFeatures />
        <QueryExample />
        <UseCases />
        <FaqSection />
      </main>
    </Layout>
  );
}
