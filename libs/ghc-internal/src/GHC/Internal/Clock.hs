{-# LANGUAGE Trustworthy #-}
{-# LANGUAGE NoImplicitPrelude #-}

-- The non-JavaScript branch of Clock.hsc, with its preprocessing done by hand (hsc has no
-- C preprocessor pass for .hsc files). getMonotonicNSec is implemented by the runtime.
module GHC.Internal.Clock
    ( getMonotonicTime
    , getMonotonicTimeNSec
    ) where

import GHC.Internal.Base
import GHC.Internal.Real
import GHC.Internal.Word
import GHC.Internal.Float () -- for Num Double instance

getMonotonicTime :: IO Double
getMonotonicTime = do
  w <- getMonotonicTimeNSec
  return (fromIntegral w / 1000000000)

foreign import ccall unsafe "getMonotonicNSec"
    getMonotonicTimeNSec :: IO Word64
