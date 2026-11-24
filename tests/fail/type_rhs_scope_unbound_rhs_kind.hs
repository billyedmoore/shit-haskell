{-# LANGUAGE PolyKinds #-}
import Data.Proxy
type T a = Proxy (a :: k)
main :: IO ()
main = pure ()
