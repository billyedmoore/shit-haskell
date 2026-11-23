{-# LANGUAGE StandaloneKindSignatures, PolyKinds #-}
import Data.Kind
import Data.Proxy
type T :: forall k. k -> Type
data T a = T (Proxy (a :: k))
main :: IO ()
main = pure ()
