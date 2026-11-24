{-# LANGUAGE NoDataKinds #-}

import Data.Proxy
import DiagnosticPromotionOrigin
value :: Proxy (L ':*: L)
value = Proxy

builtin :: Proxy (Int ': Nil)
builtin = Proxy

main :: IO ()
main = pure ()
