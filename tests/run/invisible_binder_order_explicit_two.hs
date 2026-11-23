{-# LANGUAGE PolyKinds, StandaloneKindSignatures, TypeAbstractions, NoTypeApplications, TypeOperators, RankNTypes #-}
import Data.Kind (Type)
type T :: forall k j. k -> j -> Type
data T @k @j a b = MkT
main :: IO ()
main = pure ()
