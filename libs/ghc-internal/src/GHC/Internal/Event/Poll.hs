{-# LANGUAGE NoImplicitPrelude #-}

-- hsc stand-in for GHC's Poll.hsc. The real backend calls poll(2) through FFI with struct layouts
-- that hsc2hs computes from the C headers; hsc does not run hsc2hs. Without it the event manager is
-- unavailable, which hsc's IO does not depend on.
module GHC.Internal.Event.Poll
    (
      new
    , available
    ) where

import GHC.Internal.Base
import qualified GHC.Internal.Event.Internal as E

new :: IO E.Backend
new = errorWithoutStackTrace "poll event backend is not available in hsc"

available :: Bool
available = False
