const path = require("path");
const CopyPlugin = require("copy-webpack-plugin");
const HtmlWebpackPlugin = require('html-webpack-plugin');
const {
  WebpackManifestPlugin
} = require('webpack-manifest-plugin');


const dist = path.resolve(__dirname, "dist");

module.exports = {
  mode: "production",
  experiments: {
    asyncWebAssembly: true,
  },
  entry: {
    index: "./assets/js/index.js",
    backend: "./assets/js/backend.js"
  },
  output: {
    path: dist,
    filename: "[name].[contenthash].js"
  },
  devServer: {
    static: dist,
  },
  plugins: [
    new CopyPlugin({
      patterns: [
        path.resolve(__dirname, "assets/static")
      ]
    }),
    new HtmlWebpackPlugin({
      title: 'Robotica',
      meta: {
        viewport: 'width=device-width, initial-scale=1, shrink-to-fit=no'
      },
      publicPath: '/',
      chunks: ['index'],
    }),
    new WebpackManifestPlugin({
      basePath: '',
      publicPath: ''
    })
  ],
  module: {
    rules: [{
      test: /\.(scss)$/,
      use: [{
          loader: 'style-loader'
        },
        {
          loader: 'css-loader'
        },
        {
          loader: 'postcss-loader',
          options: {
            postcssOptions: {
              plugins: () => [
                require('autoprefixer')
              ]
            }
          }
        },
        {
          loader: 'sass-loader'
        }
      ]
    }]
  }
}