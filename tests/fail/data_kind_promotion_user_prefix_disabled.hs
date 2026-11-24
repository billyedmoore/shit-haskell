{-# LANGUAGE NoDataKinds #-}

import Data.Proxy
import DiagnosticPromotionOrigin
value :: Proxy ('(:*:) L L)
value = Proxy

main :: IO ()
main = pure ()
