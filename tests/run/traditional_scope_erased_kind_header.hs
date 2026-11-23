{-# LANGUAGE PolyKinds, TypeApplications, TypeAbstractions, StandaloneKindSignatures #-}
import Data.Kind (Type)
import Data.Proxy (Proxy)

type T :: forall k. k -> Type
data T @k a = T (Proxy (a :: k))

main :: IO ()
main = pure ()
