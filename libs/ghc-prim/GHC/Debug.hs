{-# LANGUAGE MagicHash, NoImplicitPrelude, UnboxedTuples #-}

-- hsc stand-in for GHC's GHC.Debug. GHC's version writes through the C runtime's debug
-- output, which hsc does not have. Its callers are compiled out unless the library's own
-- debug-dump switch is on (c_DEBUG_DUMP is False), so these functions do nothing.
module GHC.Debug ( debugLn, debugErrLn ) where

import GHC.Types

debugLn :: [Char] -> IO ()
debugLn _ = IO (\s -> (# s, () #))

debugErrLn :: [Char] -> IO ()
debugErrLn _ = IO (\s -> (# s, () #))
