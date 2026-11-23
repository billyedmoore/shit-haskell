{-# LANGUAGE TypeFamilies, PartialTypeSignatures, PolyKinds #-}
import Data.Proxy
type family T a where
  T a = Proxy (a :: _)
main :: IO ()
main = pure ()
