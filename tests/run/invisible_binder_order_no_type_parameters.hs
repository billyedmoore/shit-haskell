{-# LANGUAGE PolyKinds, StandaloneKindSignatures, TypeAbstractions, NoTypeApplications, TypeOperators, RankNTypes #-}
import Data.Kind (Type)
type T :: forall k. Type
data T @k = MkT
main :: IO ()
main = pure ()
