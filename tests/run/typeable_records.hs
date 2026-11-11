{-# LANGUAGE CPP, RankNTypes, DataKinds #-}
import Data.Typeable
import Control.Concurrent (getNumCapabilities)
import GHC.Clock (getMonotonicTimeNSec)
import qualified Data.Map as M

-- A comment that ends in a backslash: only the physical line splices \\

data Opts m = Opts { lexeme :: forall a. m a -> m a, label :: String }

mkOpts :: Monad m => Opts m
mkOpts = Opts { lexeme = lexeme, label = "base" }
  where lexeme p = do { x <- p; return x }

data Fold b = Fold { single :: forall t. Show t => t -> b, count :: Int }

newtype Timeout = Timeout Integer

pairs :: [(Int, (Int, Int, Int))] -> [Int]
pairs xs = [v + k | (,) v (_, _, k) <- xs]

main :: IO ()
main = do
  print (typeOf (Just 'c'), typeOf (Left 3 :: Either Int Bool))
  print (typeRep (Proxy :: Proxy Maybe), typeRep (Proxy :: Proxy (Either Int)))
  print (typeOf [(1 :: Int, "s")], typeOf (), typeOf ((), 'x', True))
  print (typeOf (undefined :: Int -> IO ()), typeRep (Proxy :: Proxy Timeout))
  print (typeOf (M.empty :: M.Map TypeRep [Int]))
  print (typeOf (Proxy :: Proxy 3) == typeOf (Proxy :: Proxy 3), typeOf 'a' == typeOf 'b', typeOf 'a' == typeOf ())
  print (cast (3 :: Int) :: Maybe Int, cast 'x' :: Maybe Int)
  let o = mkOpts :: Opts Maybe
  print (lexeme o (Just 1), label o)
  let f = (Fold { single = const [], count = 0 }) { single = \x -> [show x] }
  print (single f (1 :: Int), single f True, count f { count = 2 })
  print (pairs [(1, (2, 3, 4)), (10, (0, 0, 5))])
  n <- getNumCapabilities
  t0 <- getMonotonicTimeNSec
  t1 <- getMonotonicTimeNSec
  print (n, t1 >= t0)
