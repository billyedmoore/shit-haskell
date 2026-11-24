{-# LANGUAGE PolyKinds, TypeAbstractions, NoTypeApplications, RankNTypes, StandaloneKindSignatures #-}
import Data.Kind (Type, Constraint)
import qualified Data.Kind as Kind
type T = forall (_ :: Type). Int
main :: IO ()
main = pure ()
