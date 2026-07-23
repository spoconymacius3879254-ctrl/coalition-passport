const githubPages = process.env.GITHUB_ACTIONS === "true";

/** @type {import('next').NextConfig} */
const nextConfig = {
  basePath: githubPages ? "/coalition-passport" : "",
  output: "export",
  poweredByHeader: false,
  reactStrictMode: true,
  trailingSlash: true,
  turbopack: {
    root: new URL(".", import.meta.url).pathname,
  },
};

export default nextConfig;
