{-# LANGUAGE NoDataKinds #-}

import Data.Proxy
import qualified DiagnosticPromotionOrigin as P
value :: Proxy P.Leaf
value = Proxy

main :: IO ()
main = pure ()
