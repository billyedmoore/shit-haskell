import qualified Data.Map as M
import qualified Data.Map.Strict as MS
import qualified Data.Set as S
import qualified Data.IntMap as IM
import qualified Data.Sequence as Seq
import Data.Sequence (Seq ((:<|), (:|>)), (|>), (<|))
import Data.Tree
import Data.Foldable (toList)

main :: IO ()
main = do
  let m = M.fromListWith (++) [(k `mod` 5, [k]) | k <- [1 .. 20 :: Int]]
  print m
  print (M.lookup 3 m, M.findWithDefault [] 7 m, M.keys m, M.elems (M.map length m))
  print (M.toList (M.unionWith (+) (M.fromList [(1, 1), (2, 2)]) (M.fromList [(2, 10), (3, 30 :: Int)])))
  print (M.foldrWithKey (\k v acc -> k + sum v + acc) 0 m)
  print (fst <$> M.lookupMin m, fst <$> M.lookupMax m, M.split 2 (M.fromList [(i, ()) | i <- [1 .. 4 :: Int]]))
  let ms = foldr (\k -> MS.insertWith (+) (k `mod` 3) k) MS.empty [1 .. 100 :: Int]
  print ms
  print (M.alter (fmap (* 2)) 1 (M.fromList [(1, 5), (2, 6 :: Int)]), M.adjust negate 2 (M.fromList [(1, 5), (2, 6 :: Int)]))
  let s = S.fromList "mississippi"
  print (s, S.member 's' s, S.size s, S.toList (S.map succ s), S.union s (S.fromList "abc"))
  print (S.difference (S.fromList [1 .. 10 :: Int]) (S.fromList [2, 4 .. 10]), S.partition even (S.fromList [1 .. 9 :: Int]))
  let im = IM.fromList [(i * 7 `mod` 11, i) | i <- [1 .. 10 :: Int]]
  print (im, IM.lookup 3 im, IM.findMin im, IM.toDescList im)
  let q = Seq.fromList [1 .. 5 :: Int] |> 6
  print (q, 0 <| q, Seq.length q, Seq.index q 2, Seq.reverse q, Seq.update 0 100 q)
  case q of
    a :<| (rest :|> z) -> print (a, toList rest, z)
    _ -> putStrLn "short"
  let t = Node "root" [Node "a" [Node "a1" [], Node "a2" []], Node "b" []]
  putStr (drawTree t)
  print (flatten t, levels t, length t)
  print (show m == show (M.fromList (M.toList m)), M.valid m)
