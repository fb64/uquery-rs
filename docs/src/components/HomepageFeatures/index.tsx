import clsx from 'clsx';
import Heading from '@theme/Heading';
import ThemedImage from '@theme/ThemedImage';
import useBaseUrl from '@docusaurus/useBaseUrl';
import type {JSX} from 'react';
import styles from './styles.module.css';

type FeatureItem = {
  title: string;
  // Rendered as <img>: inlining breaks the masks and <use> refs in some logos.
  img: {light: string; dark: string; alt: string};
  description: JSX.Element;
};

const FeatureList: FeatureItem[] = [
  {
    title: 'Lightweight & Fast',
    img: {
      light: '/img/uquery.svg',
      dark: '/img/uquery-dark.svg',
      alt: 'µQuery logo: a pink µ and a green Q split by a yellow lightning bolt',
    },
    description: (
      <>
        µQuery was designed to be deployed on a serverless infrastructure.
      </>
    ),
  },
  {
    title: 'Focus on performance',
    img: {light: '/img/rust-logo-blk.svg', dark: '/img/rust-logo-white.svg', alt: 'Rust logo'},
    description: (
      <>
        Written in Rust and optimized for performance. Query results are directly streamed back to the client.
      </>
    ),
  },
  {
    title: 'Powered by DuckDB',
    img: {light: '/img/DuckDB_Logo-horizontal.svg', dark: '/img/DuckDB_Logo-horizontal-dark.svg', alt: 'DuckDB logo'},
    description: (
      <>
        Embeds DuckDB engine to use rich SQL features to query your data.
      </>
    ),
  },
];

function Feature({title, img, description}: FeatureItem) {
  return (
    <div className={clsx('col col--4')}>
      <div className={styles.featureLogo}>
        <ThemedImage
          className={styles.featureImg}
          alt={img.alt}
          sources={{light: useBaseUrl(img.light), dark: useBaseUrl(img.dark)}}
        />
      </div>
      <div className="text--center padding-horiz--md">
        <Heading as="h3">{title}</Heading>
        <p>{description}</p>
      </div>
    </div>
  );
}

export default function HomepageFeatures(): JSX.Element {
  return (
    <section className={styles.features}>
      <div className="container">
        <div className="row">
          {FeatureList.map((props, idx) => (
            <Feature key={idx} {...props} />
          ))}
        </div>
      </div>
    </section>
  );
}
