{-# LANGUAGE PartialTypeSignatures, PolyKinds #-}
import Data.Proxy
type T = Proxy (Int :: _)
main :: IO ()
main = pure ()
