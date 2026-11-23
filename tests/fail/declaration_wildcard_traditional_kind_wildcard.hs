{-# LANGUAGE PartialTypeSignatures, PolyKinds #-}
import Data.Proxy (Proxy)

data T a = T (Proxy (a :: _))

main :: IO ()
main = pure ()
