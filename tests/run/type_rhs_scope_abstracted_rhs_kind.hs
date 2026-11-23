{-# LANGUAGE PolyKinds, TypeAbstractions, TypeApplications, StandaloneKindSignatures #-}
import Data.Kind
import Data.Proxy
type T :: forall k. k -> Type
type T @k a = Proxy (a :: k)
main :: IO ()
main = print (Proxy :: T Bool)
