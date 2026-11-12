{-# LANGUAGE DerivingVia, DataKinds, KindSignatures, StandaloneDeriving #-}
import Data.Monoid (Sum(..), Any(..))
newtype Wrap (n :: Bool) a = Wrap a
newtype And f a = And a
class Size a where size :: a -> Int
instance Size (And f a) where size _ = 7
newtype T = T Int deriving Size via (Wrap 'True `And` T)
newtype U = U Int deriving Size via (And Maybe U)
data D = D Int Bool deriving Size via (And [] D)
newtype Total = Total Int deriving (Semigroup, Monoid) via (Sum Int)
  deriving stock Show
newtype Flag = Flag Bool deriving (Semigroup, Monoid) via Any
deriving stock instance Show Flag
data E = E
deriving via (And Maybe E) instance Size E
main = do
  print (size (T 1), size (U 2), size (D 1 True), size E)
  print (mconcat [Total 1, Total 2, Total 3], mempty :: Total)
  print (Flag False <> Flag True, mempty :: Flag)
