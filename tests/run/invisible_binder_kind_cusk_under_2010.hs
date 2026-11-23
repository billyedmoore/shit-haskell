{-# LANGUAGE PolyKinds, TypeAbstractions, NoTypeApplications, RankNTypes, Haskell2010, EmptyDataDecls #-}
import Data.Kind (Type, Constraint)
import qualified Data.Kind as Kind
data T @(k :: Type) (a :: k) = T
main :: IO ()
main = pure ()
