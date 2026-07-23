/** @type {import('next').NextConfig} */
const nextConfig = {
  poweredByHeader: false,
  reactStrictMode: true,
  turbopack: {
    root: new URL(".", import.meta.url).pathname,
  },
};

export default nextConfig;
