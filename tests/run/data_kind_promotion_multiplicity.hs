{-# LANGUAGE NoDataKinds #-}

import Data.Proxy
import GHC.Types (Multiplicity(..))

value :: Proxy 'Many
value = Proxy

main :: IO ()
main = pure ()
