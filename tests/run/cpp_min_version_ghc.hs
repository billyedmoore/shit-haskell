{-# LANGUAGE CPP #-}
-- GHC predefines the GHC version test macro (from ghcversion.h), as ghc-lib-parser uses it.
module Main where

main :: IO ()
main = do
#if MIN_VERSION_GLASGOW_HASKELL(9,3,0,0)
  putStrLn "at least 9.3"
#else
  putStrLn "older than 9.3"
#endif
#if MIN_VERSION_GLASGOW_HASKELL(9,12,2,0)
  putStrLn "at least 9.12.2"
#endif
#if MIN_VERSION_GLASGOW_HASKELL(9,12,3,0)
  putStrLn "at least 9.12.3"
#else
  putStrLn "older than 9.12.3"
#endif
#if MIN_VERSION_GLASGOW_HASKELL(9,14,0,0)
  putStrLn "at least 9.14"
#endif
  print (__GLASGOW_HASKELL_PATCHLEVEL1__ :: Int)
