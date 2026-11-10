{-# LANGUAGE NoImplicitPrelude #-}

-- hsc stand-in for GHC's EPoll.hsc. The real backend calls epoll_create1/epoll_ctl/epoll_wait and
-- reads struct layouts that hsc2hs computes from the C headers; hsc does not run hsc2hs. Without this
-- backend the event manager is unavailable, which hsc's IO does not depend on.
module GHC.Internal.Event.EPoll
    (
      new
    , available
    ) where

import GHC.Internal.Base
import qualified GHC.Internal.Event.Internal as E

new :: IO E.Backend
new = errorWithoutStackTrace "epoll event backend is not available in hsc"

available :: Bool
available = False
