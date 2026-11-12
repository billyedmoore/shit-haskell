-- System.Info's values come from ghcplatform.h (Cabal-syntax uses them).
module Main where

import System.Info (os, arch, compilerName, fullCompilerVersion)
import Data.Version (showVersion)

main :: IO ()
main = print (os, arch, compilerName, showVersion fullCompilerVersion)
