{-# LANGUAGE PolyKinds #-}
import Data.Proxy
data T a = T (Proxy (a :: k))
main :: IO ()
main = pure ()
